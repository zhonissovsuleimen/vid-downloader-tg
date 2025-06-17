use crate::downloader::{downloader_error::DownloaderError, playlist::media_playlist::MediaPlaylist};
use tokio::process::Command;

pub struct MasterPlaylist {
  pub resolution: String,
  video_media_playlist: Option<MediaPlaylist>,
  audio_media_playlist: Option<MediaPlaylist>,
  video_media_url: String,
  audio_media_url: Option<String>
}

impl MasterPlaylist {
  pub async fn from_urls(video_url: String, audio_url: Option<String>) -> Result<Self, DownloaderError> {
    Ok(MasterPlaylist {
      resolution: String::new(),
      video_media_playlist: None,
      audio_media_playlist: None,
      video_media_url: video_url,
      audio_media_url: audio_url
    })
  }

  pub async fn download(&mut self) -> Result<String, DownloaderError> {
    self.video_media_playlist = Some(MediaPlaylist::from_url(&self.video_media_url).await?);
    let video_media_playlist = self.video_media_playlist.as_ref().unwrap();
    let video_bytes = video_media_playlist.get_byte_data();

    let video_name = video_media_playlist.name.split('/').last().unwrap().split('.').next().unwrap().to_string();
    let output_name = format!("{}_{}.mp4", video_name, self.resolution);

    tokio::fs::write(video_name.clone(), video_bytes).await.map_err(|_| DownloaderError::IOError)?;

    match &self.audio_media_url {
      Some(audio_media_url) => {
        self.audio_media_playlist = Some(MediaPlaylist::from_url(&audio_media_url).await?);
        let audio_media_playlist = self.audio_media_playlist.as_ref().unwrap();
        let audio_bytes = audio_media_playlist.get_byte_data();

        let audio_name = audio_media_playlist.name.split('/').last().unwrap().split('.').next().unwrap().to_string();
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

        tokio::fs::remove_file(audio_name).await.map_err(|_| DownloaderError::IOError)?;
      }
      None => {
        Command::new("ffmpeg")
          .args(&["-i", &video_name])
          .args(["-c", "copy"])
          .arg("-y")
          .arg(&output_name)
          .output()
          .await
          .map_err(|_| DownloaderError::FfmpegError)?;
      }
    }

    tokio::fs::remove_file(video_name).await.map_err(|_| DownloaderError::IOError)?;

    Ok(output_name)
  }
}
