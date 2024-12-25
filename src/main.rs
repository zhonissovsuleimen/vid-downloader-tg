mod downloader;

// mod bot_wrapper;
use downloader::Downloader;
use teloxide::{prelude::*, types::InputFile};

#[tokio::main]
async fn main() {
  let bot = Bot::from_env();

  teloxide::repl(bot, answer).await;
}

async fn answer(bot: Bot, msg: Message) -> ResponseResult<()> {
  if let Some(input) = msg.text() {
    match input.starts_with('/') {
      true => match input {
        "/platforms" => send_platforms(bot, msg).await?,
        _ => send_help(bot, msg).await?,
      },
      false => {
        send_video(bot, msg).await?;
      }
    }
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
