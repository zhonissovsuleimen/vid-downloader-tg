mod downloader;

// mod bot_wrapper;
use downloader::Downloader;
use teloxide::{prelude::*, types::InputFile};
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

  teloxide::repl(bot, answer).await;
}

async fn answer(bot: Bot, msg: Message) -> ResponseResult<()> {
  if let Some(input) = msg.text() {
    info!("Received text message: {input}");
    match input.starts_with('/') {
      true => match input {
        "/platforms" => send_platforms(bot, msg).await?,
        _ => send_help(bot, msg).await?,
      },
      false => {
        send_video(bot, msg).await?;
      }
    }
    info!("Handled user message");
  }
  Ok(())
}

async fn send_help(bot: Bot, msg: Message) -> ResponseResult<()> {
  const HELP: &str = "To download a video, send the video URL to me. I will download the video and send it back to you.\n\n\
    Commands:\n\
    /help - Show this message\n\
    /platforms - Show supported platforms\n\
    ";
  bot.send_message(msg.chat.id, HELP).await?;
  Ok(())
}

async fn send_platforms(bot: Bot, msg: Message) -> ResponseResult<()> {
  const PLATFORMS: &str = "Twitter / X [Videos]\
    ";
  bot.send_message(msg.chat.id, PLATFORMS).await?;
  Ok(())
}

async fn send_video(bot: Bot, msg: Message) -> ResponseResult<()> {
  let downloader = Downloader::new();
  match downloader.download(msg.text().unwrap()).await {
    Ok(path) => {
      bot.send_video(msg.chat.id, InputFile::file(path.clone())).await?;
      tokio::fs::remove_file(path).await?;
      },
    Err(e) => {
      bot
        .send_message(msg.chat.id, format!("Failed to download video ({})", e.to_string()))
        .await?;
    }
  };
  Ok(())
}
