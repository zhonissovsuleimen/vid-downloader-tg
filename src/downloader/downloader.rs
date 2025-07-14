use headless_chrome::{Browser, LaunchOptions};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use super::{downloader_error::DownloaderError, playlist::variant_playlist::VariantPlaylist};

pub trait PlatformDownloader {
  async fn download(browser: &Browser, url: &str) -> Result<String, DownloaderError>;
  async fn get_variant_playlist(browser: &Browser, url: &str) -> Result<VariantPlaylist, DownloaderError>;
  fn validate_url(url: &str) -> Result<(), DownloaderError>;
}

pub struct Downloader {
  pub browser: Browser,
}

impl Downloader {
  pub fn new() -> Self {
    let browser = Browser::new(LaunchOptions {
      idle_browser_timeout: Duration::from_secs(1e7 as u64),
      args: vec![
        std::ffi::OsStr::new("--incognito"),
        std::ffi::OsStr::new("--mute-audio")
      ],
      ..Default::default()
    })
    .unwrap();

    let pid = browser.get_process_id().unwrap();
    let _ = ctrlc::set_handler(move || {
      use std::process::Command;

      #[cfg(target_os = "windows")]
      {
        let _ = Command::new("taskkill").args(&["/PID", &pid.to_string(), "/T", "/F"]).output();
      }
      #[cfg(target_os = "linux")]
      {
        let _ = Command::new("kill").args(&["-9", &format!("-{}", pid)]).output();
      }
      std::process::exit(0);
    });

    Self { browser }
  }
}
