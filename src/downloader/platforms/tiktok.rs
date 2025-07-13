use headless_chrome::{
  browser::{
    tab::{RequestInterceptor, RequestPausedDecision},
    transport::{SessionId, Transport},
  },
  protocol::cdp::{
    Fetch::{events::RequestPausedEvent, RequestPattern, RequestStage},
    Network::ResourceType,
    Target::CreateTarget,
  },
  Browser,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::downloader::{
  downloader::PlatformDownloader, downloader_error::DownloaderError, playlist::variant_playlist::VariantPlaylist,
};

pub struct TiktokDownloader {}

impl PlatformDownloader for TiktokDownloader {
  async fn download(browser: &Browser, url: &str) -> Result<String, DownloaderError> {
    let target = get_initial_tab_create_target();
    let tab = browser.new_tab_with_options(target)?;
    let intercepted_url = Arc::new(Mutex::new(String::new()));
    let intercepted_cookie = Arc::new(Mutex::new(String::new()));
    let interceptor = get_interceptor(intercepted_url.clone(), intercepted_cookie.clone());

    tab.enable_fetch(Some(&vec![get_request_pattern()]), None)?;
    tab.enable_request_interception(interceptor)?;
    tab.navigate_to(url)?;

    let mut found = false;
    let mut timeout = 10.0 as f32;
    while !found && timeout >= 0.0 {
      found = !intercepted_url.lock().await.is_empty();
      tokio::time::sleep(Duration::from_millis(100)).await;
      timeout -= 0.1;
    }
    let _ = tab.close(false);

    if !found {
      return Err(DownloaderError::FetchError);
    }

    let client = reqwest::Client::new();

    let url_mutex_guard = intercepted_url.lock().await.to_owned();
    let cookie_mutex_guard = intercepted_cookie.lock().await.to_owned();
    let response = client
      .get(url_mutex_guard)
      .header("User-Agent", r"Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0")
      .header("Referer", r"https://www.tiktok.com/")
      .header("Cookie", cookie_mutex_guard)
      .send()
      .await
      .map_err(|_| DownloaderError::FetchError)?;

    let bytes = response.bytes().await.map_err(|_| DownloaderError::FetchError)?;

    let mut output_name = url.split('/').filter(|s| !s.is_empty()).last().unwrap_or("video").to_string();
    if output_name.contains('?') {
      output_name = output_name.split('?').next().unwrap().to_string();
    }
    output_name.push_str(".mp4");

    tokio::fs::write(&output_name, bytes).await.map_err(|_| DownloaderError::IOError)?;

    Ok(output_name)
  }

  async fn get_variant_playlist(_browser: &Browser, _url: &str) -> Result<VariantPlaylist, DownloaderError> {
    //since we are downloading the .mp4 file directly
    Err(DownloaderError::NoMasterPlaylistError)
  }

  fn validate_url(url: &str) -> Result<(), DownloaderError> {
    let tiktok_regex = regex::Regex::new(r"https:\/\/(www\.)?tiktok.com\/@.+\/video\/\d+(\?.*)?").unwrap();
    let tiktok_short_regex = regex::Regex::new(r"https:\/\/(www\.)?\w+\.tiktok\.com\/[^@]\w+").unwrap();

    if !tiktok_regex.is_match(url) && !tiktok_short_regex.is_match(url) {
      return Err(DownloaderError::UnsupportedPlatformError);
    }

    Ok(())
  }
}

fn get_interceptor(url: Arc<Mutex<String>>, cookie: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
  Arc::new(move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
    let request = event.params.request.clone();

    let mut url_mutex_guard = url.blocking_lock();
    let mut cookie_mutex_guard = cookie.blocking_lock();
    if request.url.contains("mime_type=video_mp4") && url_mutex_guard.is_empty() {
      *url_mutex_guard = request.url;

      if let Some(header) = request.headers.0 {
        if let Some(cookie_header) = header.get("Cookie") {
          *cookie_mutex_guard = cookie_header.clone().take().to_string();
        }
      }
    }

    RequestPausedDecision::Continue(None)
  })
}

fn get_request_pattern() -> RequestPattern {
  RequestPattern {
    url_pattern: Some("https://v16-webapp-prime.tiktok.com/video/*".to_string()),
    resource_Type: Some(ResourceType::Xhr),
    request_stage: Some(RequestStage::Request),
  }
}

fn get_initial_tab_create_target() -> CreateTarget {
  CreateTarget {
    url: "about::blank".to_string(),
    width: None,
    height: None,
    browser_context_id: None,
    enable_begin_frame_control: None,
    new_window: Some(true),
    background: Some(true),
  }
}
