mod downloader;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// mod bot_wrapper;
use downloader::{playlist::variant_playlist::VariantPlaylist, Downloader};
use teloxide::{
  dispatching::dialogue::GetChatId,
  prelude::*,
  types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, MediaKind::*, MessageEntityKind::*, MessageId, MessageKind::*},
  RequestError,
};
use tracing::info;
use tracing_subscriber::{self, fmt::format::FmtSpan};

struct State {
  downloader: Downloader,
  variants: HashMap<(ChatId, MessageId), VariantPlaylist>,
}

#[tokio::main]
async fn main() {
  tracing_subscriber::fmt()
    .compact()
    .with_ansi(false)
    .with_file(false)
    .with_level(true)
    .with_line_number(false)
    .with_span_events(FmtSpan::FULL)
    .with_target(false)
    .with_thread_ids(true)
    .with_thread_names(false)
    .init();

  info!("Starting telegram bot...");
  let bot = Bot::from_env();

  let state = State { downloader: Downloader::new(), variants: HashMap::new() };

  let handler = dptree::entry()
    .branch(Update::filter_message().endpoint(message_handler))
    .branch(Update::filter_callback_query().endpoint(callback_query_handler));

  Dispatcher::builder(bot, handler)
    .dependencies(dptree::deps![Arc::new(Mutex::new(state))])
    .enable_ctrlc_handler()
    .build()
    .dispatch()
    .await;
}

async fn message_handler(bot: Bot, msg: Message, state: Arc<Mutex<State>>) -> ResponseResult<()> {
  match &msg.kind {
    Common(message_common) => match &message_common.media_kind {
      Text(media_text) => {
        let is_command = media_text.entities.iter().any(|e| e.kind == BotCommand);
        let is_link = media_text.entities.iter().any(|e| e.kind == Url);

        match media_text.text.as_str() {
          url if is_link => send_video(bot, msg.chat.id, msg.id, url, state).await?,
          "/platforms" if is_command => send_platforms(bot, msg.chat.id).await?,
          _ => send_help(bot, msg.chat.id).await?,
        }
        info!("Handled user message");
      }
      _ => {}
    },
    _ => {}
  }
  Ok(())
}

async fn send_help(bot: Bot, chat_id: ChatId) -> ResponseResult<()> {
  const HELP: &str = "To download a video, send the video URL to me. I will download the video and send it back to you.\n\n\
    Commands:\n\
    /help - Show this message\n\
    /platforms - Show supported platforms\n\
    ";
  bot.send_message(chat_id, HELP).await?;
  Ok(())
}

async fn send_platforms(bot: Bot, chat_id: ChatId) -> ResponseResult<()> {
  const PLATFORMS: &str = "\
  Twitter / X [Videos]\
  ";
  bot.send_message(chat_id, PLATFORMS).await?;
  Ok(())
}

async fn send_video(bot: Bot, chat_id: ChatId, msg_id: MessageId, url: &str, state: Arc<Mutex<State>>) -> ResponseResult<()> {
  let mut state_guard = state.lock().await;

  match state_guard.downloader.get_variant_playlist(url).await {
    Ok(variant_playlist) => {
      let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];
      for (i, playlist) in variant_playlist.master_playlists.iter().enumerate() {
        let key = format!("{msg_id} {i}");
        keyboard.push(vec![InlineKeyboardButton::callback(&playlist.resolution, key)]);
      }

      state_guard.variants.insert((chat_id, msg_id), variant_playlist);
      bot.send_message(chat_id, "Select a resolution to download").reply_markup(InlineKeyboardMarkup::new(keyboard)).await?;
    }
    Err(e) => {
      bot.send_message(chat_id, format!("Failed to download video ({})", e.to_string())).await?;
    }
  };
  Ok(())
}

async fn callback_query_handler(bot: Bot, query: CallbackQuery, state: Arc<Mutex<State>>) -> ResponseResult<()> {
  let prev_msg_id = &query.message.as_ref().unwrap().id();
  let chat_id = query.chat_id().unwrap();

  if let Some(ref callback_data) = query.data {
    bot.answer_callback_query(&query.id).await?;
    bot.delete_message(chat_id, *prev_msg_id).await?;

    let (msg_id, resolution_index) = callback_data.split_once(' ').unwrap();
    let msg_id = MessageId(msg_id.parse::<i32>().unwrap());
    let resolution_index = resolution_index.parse::<usize>().unwrap();

    let variants = &mut state.lock().await.variants;
    let variant_playlist = variants.get_mut(&(chat_id, msg_id)).unwrap();
    let master_playlist = &mut variant_playlist.master_playlists[resolution_index];

    let path = master_playlist
      .download()
      .await
      .map_err(|e| RequestError::from(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
    bot.send_video(chat_id, InputFile::file(&path)).await?;

    variants.remove(&(chat_id, msg_id));
    tokio::fs::remove_file(&path).await?;
  }

  Ok(())
}
