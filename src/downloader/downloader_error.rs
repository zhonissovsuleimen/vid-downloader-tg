use std::fmt::Display;

pub enum DownloaderError {
  InvalidInputError,
  UnsupportedPlatformError,
  FetchError,
  MediaPlaylistExtractError,
  IOError,
  FfmpegError,
  OtherError(String),
}

impl From<anyhow::Error> for DownloaderError {
  fn from(e: anyhow::Error) -> Self {
    DownloaderError::OtherError(e.to_string())
  }
}

impl Display for DownloaderError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DownloaderError::InvalidInputError => write!(f, "Invalid input"),
      DownloaderError::UnsupportedPlatformError => write!(f, "Platform not supported"),
      DownloaderError::FetchError => write!(f, "Failed to fetch data from external source"),
      DownloaderError::MediaPlaylistExtractError => write!(f, "Failed to extract media playlist"),
      DownloaderError::IOError => write!(f, "Failed to perform IO operation"),
      DownloaderError::FfmpegError => write!(f, "Failed to execute ffmpeg command"),
      DownloaderError::OtherError(e) => write!(f, "Error: {}", e),
    }
  }
}
