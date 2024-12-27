mod downloader;

// mod bot_wrapper;
use downloader::Downloader;
use teloxide::{
  dispatching::dialogue::GetChatId,
  prelude::*,
  types::{InlineKeyboardButton, InlineKeyboardMarkup, InputFile, MediaKind::*, MessageEntityKind::*, MessageKind::*},
};
use tracing::info;
use tracing_subscriber::{self, fmt::format::FmtSpan};

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

  let handler = dptree::entry()
    .branch(Update::filter_message().endpoint(message_handler))
    .branch(Update::filter_callback_query().endpoint(callback_query_handler));

  Dispatcher::builder(bot, handler).enable_ctrlc_handler().build().dispatch().await;
}

async fn message_handler(bot: Bot, msg: Message) -> ResponseResult<()> {
  match &msg.kind {
    Common(message_common) => match &message_common.media_kind {
      Text(media_text) => {
        let is_command = media_text.entities.iter().any(|e| e.kind == BotCommand);
        let is_link = media_text.entities.iter().any(|e| e.kind == Url);

        match media_text.text.as_str() {
          url if is_link => send_video(bot, msg.chat.id, url).await?,
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

async fn send_video(bot: Bot, chat_id: ChatId, url: &str) -> ResponseResult<()> {
  let mut downloader = Downloader::new();
  match downloader.download(url).await {
    Ok(_) => {
      let mut keyboard: Vec<Vec<InlineKeyboardButton>> = vec![];

      for playlist in downloader.variant_playlist.unwrap().master_playlists {
        let file_name =
          playlist.write().await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Failed to write playlist"))?;
        keyboard.push(vec![InlineKeyboardButton::callback(playlist.resolution, file_name)]);
      }

      bot.send_message(chat_id, "Select a resolution to download").reply_markup(InlineKeyboardMarkup::new(keyboard)).await?;
    }
    Err(e) => {
      bot.send_message(chat_id, format!("Failed to download video ({})", e.to_string())).await?;
    }
  };
  Ok(())
}

async fn callback_query_handler(bot: Bot, query: CallbackQuery) -> ResponseResult<()> {
  if let Some(ref filename) = query.data {
    bot.answer_callback_query(&query.id).await?;
    let chat_id = query.chat_id().unwrap();
    let message_id = query.message.unwrap().id();

    bot.delete_message(chat_id, message_id).await?;
    bot.send_video(chat_id, InputFile::file(filename.clone())).await?;
  }

  Ok(())
}
