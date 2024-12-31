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

pub struct TwitterDownloader {}

impl PlatformDownloader for TwitterDownloader {
  async fn download(_browser: &Browser, _url: &str) -> Result<String, DownloaderError> {
    Err(DownloaderError::OtherError("Twitter downloader supports variant playlist. Please use get_variant_playlist function".into()))
  }

  async fn get_variant_playlist(browser: &Browser, url: &str) -> Result<VariantPlaylist, DownloaderError> {
    let target = get_initial_tab_create_target();
    let tab = browser.new_tab_with_options(target)?;
    let intercepted_url = Arc::new(Mutex::new(String::new()));
    let interceptor = get_interceptor(intercepted_url.clone());

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

    let variant_playlist_url = intercepted_url.lock().await.to_owned();
    VariantPlaylist::from_url(&variant_playlist_url).await
  }

  fn validate_url(url: &str) -> Result<(), DownloaderError> {
    let twitter_regex = regex::Regex::new(r"https:\/\/(www\.)?(twitter|x).com\/.+\/status\/\d+(\?.*)?").unwrap();

    if !twitter_regex.is_match(url) {
      return Err(DownloaderError::UnsupportedPlatformError);
    }

    Ok(())
  }
}

fn get_interceptor(intercepted_url: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
  Arc::new(move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
    let request = event.params.request.clone();

    if request.url.contains("tag=") && intercepted_url.blocking_lock().is_empty() {
      let mut mutex_guard = intercepted_url.blocking_lock();
      let pure_url = match request.url.find('?') {
        Some(index) => request.url[..index].to_string(),
        None => request.url,
      };

      *mutex_guard = pure_url;
    }

    RequestPausedDecision::Continue(None)
  })
}

fn get_request_pattern() -> RequestPattern {
  RequestPattern {
    url_pattern: Some("https://video.twimg.com/*_video/*".to_string()),
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
