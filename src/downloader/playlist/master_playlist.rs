use crate::downloader::downloader_error::DownloaderError;
use tokio::process::Command;
use tracing::info;

use super::media_playlist::MediaPlaylist;

pub struct MasterPlaylist {
  pub resolution: String,
  pub video_media_playlist: MediaPlaylist,
  pub audio_media_playlist: MediaPlaylist,
}

impl MasterPlaylist {
  pub async fn from_urls(video_url: &str, audio_url: &str) -> Result<Self, DownloaderError> {
    info!("Fetching master playlist from: {} and {}", video_url, audio_url);
    let video_media_playlist = MediaPlaylist::from_url(video_url).await?;
    let audio_media_playlist = MediaPlaylist::from_url(audio_url).await?;

    Ok(MasterPlaylist {
      resolution: String::new(),
      video_media_playlist: video_media_playlist,
      audio_media_playlist: audio_media_playlist,
    })
  }

  pub async fn write(&self) -> Result<String, DownloaderError> {
    let video_bytes = self.video_media_playlist.get_byte_data();
    let audio_bytes = self.audio_media_playlist.get_byte_data();

    let video_name = self.video_media_playlist.name.split('/').last().unwrap().to_string();
    let audio_name = self.audio_media_playlist.name.split('/').last().unwrap().to_string();
    let output_name = format!("{}_{}.mp4", video_name.replace(".m3u8", ""), self.resolution);
    info!("Downloading video {} with resolution: {}", output_name, self.resolution);

    tokio::fs::write(video_name.clone(), video_bytes).await.map_err(|_| DownloaderError::IOError)?;
    tokio::fs::write(audio_name.clone(), audio_bytes).await.map_err(|_| DownloaderError::IOError)?;

    Command::new("ffmpeg")
      .args(&["-i", &video_name])
      .args(&["-i", &audio_name])
      .args(["-c", "copy"])
      .arg("-y")
      .arg(&output_name)
      .output()
      .await
      .map_err(|_| DownloaderError::FfmpegError)?;

    tokio::fs::remove_file(video_name).await.map_err(|_| DownloaderError::IOError)?;
    tokio::fs::remove_file(audio_name).await.map_err(|_| DownloaderError::IOError)?;

    info!("Downloaded video {} successfully", output_name);
    Ok(output_name)
  }
}
