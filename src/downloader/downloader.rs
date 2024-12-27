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
  Browser, LaunchOptions,
};
use std::{
  sync::{Arc, Mutex},
  time::Duration,
};
use tracing::info;

use super::{downloader_error::DownloaderError, playlist::variant_playlist::{self, VariantPlaylist}};

enum Platform {
  Twitter,
  Other,
}

pub struct Downloader {
  browser: Arc<Browser>,
  request_patterns: Vec<RequestPattern>,
  pub variant_playlist: Option<VariantPlaylist>,
}

impl Downloader {
  pub fn new() -> Self {
    let browser = Arc::new(
      Browser::new(LaunchOptions {
        idle_browser_timeout: Duration::from_secs(1e7 as u64),
        args: vec![std::ffi::OsStr::new("--incognito")],
        ..Default::default()
      })
      .unwrap(),
    );
    let process_id = browser.get_process_id().unwrap();

    //hopefully killing the browser if the terminal is terminated unexpectedly
    #[cfg(target_os = "windows")]
    unsafe {
      use std::ptr::null_mut;
      use winapi::um::handleapi::CloseHandle;
      use winapi::um::jobapi2::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject};
      use winapi::um::processthreadsapi::OpenProcess;
      use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
      };

      let h_job = CreateJobObjectW(null_mut(), null_mut());
      let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: std::mem::zeroed(),
        IoInfo: std::mem::zeroed(),
        ProcessMemoryLimit: 0,
        JobMemoryLimit: 0,
        PeakProcessMemoryUsed: 0,
        PeakJobMemoryUsed: 0,
      };
      info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
      SetInformationJobObject(
        h_job,
        JobObjectExtendedLimitInformation,
        &mut info as *mut _ as *mut _,
        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
      );

      let process_handle = OpenProcess(0x001F0FFF, 0, process_id);
      AssignProcessToJobObject(h_job, process_handle);
      // Close the process handle when done
      CloseHandle(process_handle);
    }
    #[cfg(target_os = "linux")]
    {
      tokio::spawn(async move {
        info!("Waiting for ctrl-c command to kill browser");
        let _ = signal::ctrl_c().await;
        info!("Received ctrl-c command, killing browser");
        {
          let _ = Command::new("kill").args(&["-9", &process_id.to_string()]).output().await;
        }
      });
    }

    let video_pattern = RequestPattern {
      url_pattern: Some("https://video.twimg.com/*_video/*".to_string()),
      resource_Type: Some(ResourceType::Xhr),
      request_stage: Some(RequestStage::Request),
    };

    Self { browser: browser, request_patterns: vec![video_pattern], variant_playlist: None }
  }

  fn get_interceptor(result: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
    Arc::new(move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
      let request = event.params.request.clone();

      if request.url.contains("tag=") && result.lock().unwrap().is_empty() {
        let mut mutex_guard = result.lock().unwrap();
        let pure_url = match request.url.find('?') {
          Some(index) => request.url[..index].to_string(),
          None => request.url,
        };

        *mutex_guard = pure_url;
      }

      RequestPausedDecision::Continue(None)
    })
  }

  pub async fn download(&mut self, url: &str) -> Result<(), DownloaderError> {
    info!("Recieved download call for {url}");

    info!("Validating url");
    validate_url(url)?;

    let target = CreateTarget {
      url: "about::blank".to_string(),
      width: None,
      height: None,
      browser_context_id: None,
      enable_begin_frame_control: None,
      new_window: Some(true),
      background: Some(true),
    };

    let tab = self.browser.new_tab_with_options(target)?;
    let intercepted_result = Arc::new(Mutex::new(String::new()));
    let interceptor = Self::get_interceptor(intercepted_result.clone());

    tab.enable_fetch(Some(&self.request_patterns), None)?;
    tab.enable_request_interception(interceptor)?;

    tab.navigate_to(url)?;

    let mut found = false;
    let mut timeout = 10.0 as f32;
    while !found && timeout >= 0.0 {
      found = !intercepted_result.lock().unwrap().is_empty();
      tokio::time::sleep(Duration::from_millis(100)).await;
      timeout -= 0.1;
    }

    let _ = tab.close(false);

    let variant_playlist_url = intercepted_result.lock().unwrap().to_owned();
    let variant_playlist = variant_playlist::VariantPlaylist::from_url(&variant_playlist_url).await.map_err(|_| DownloaderError::FetchError)?;
    self.variant_playlist = Some(variant_playlist);
    Ok(())
  }
}

fn validate_url(url: &str) -> Result<(), DownloaderError> {
  if url.is_empty() || !url.starts_with("https://") {
    return Err(DownloaderError::InvalidInputError);
  }

  let twitter_regex = regex::Regex::new(r"https://(twitter|x).com/.+/status/\d+").unwrap();

  if !twitter_regex.is_match(url) {
    return Err(DownloaderError::UnsupportedPlatformError);
  }

  Ok(())
}