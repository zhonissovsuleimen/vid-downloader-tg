use std::fmt::Display;

pub enum DownloaderError {
  InvalidInputError,
  UnsupportedPlatformError,
  FetchError,
  NoMasterPlaylistError,
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
    use DownloaderError::*;
    match self {
      InvalidInputError => write!(f, "Invalid input"),
      UnsupportedPlatformError => write!(f, "Platform not supported"),
      FetchError => write!(f, "Failed to fetch data from external source"),
      NoMasterPlaylistError => write!(f, "No master playlist found"),
      IOError => write!(f, "Failed to perform IO operation"),
      FfmpegError => write!(f, "Failed to execute ffmpeg command"),
      OtherError(e) => write!(f, "Error: {}", e),
    }
  }
}
