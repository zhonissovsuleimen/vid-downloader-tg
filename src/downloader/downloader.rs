use futures::future;
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
use reqwest::Client;
use std::{
  sync::{Arc, Mutex},
  time::Duration,
};
use tokio::process::Command;
use tracing::info;

use super::downloader_error::DownloaderError;

enum Platform {
  Twitter,
  Other,
}

pub struct Downloader {
  browser: Arc<Browser>,

  request_patterns: Vec<RequestPattern>,
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

    Self {
      browser: browser,
      request_patterns: vec![video_pattern],
    }
  }

  fn get_interceptor(result: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
    Arc::new(
      move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
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
      },
    )
  }

  pub async fn download(&self, url: &str) -> Result<String, DownloaderError> {
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

    let master_playlist_url = intercepted_result.lock().unwrap().to_owned();

    let media_urls = get_media_playlist_urls(&master_playlist_url).await?;

    let id = std::process::id();
    let output_name = media_urls.0.split('/').last().unwrap().replace(".m3u8", ".mp4");
    let video_name = format!("video_{}_{}", id, output_name);
    let audio_name = format!("audio_{}_{}", id, output_name);

    info!("Started downloading video and audio segments");
    let segments = download_segments(media_urls.clone()).await?;

    tokio::fs::write(video_name.clone(), segments.0)
      .await
      .map_err(|_| DownloaderError::IOError)?;
    tokio::fs::write(audio_name.clone(), segments.1)
      .await
      .map_err(|_| DownloaderError::IOError)?;

    info!("Downloaded segments, executing ffmpeg command");
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

    info!("Downloaded video successfully");
    Ok(output_name)
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

async fn get_media_playlist_urls(master_playlist_url: &str) -> Result<(String, String), DownloaderError> {
  let response = reqwest::get(master_playlist_url).await;
  let lines: Vec<String>;
  match response {
    Ok(result) => {
      lines = result
        .text()
        .await
        .unwrap()
        .lines()
        .filter(|line| (line.contains("/ext_tw_video/") || line.contains("/amplify_video/")) && !line.contains("TYPE=SUBTITLES"))
        .map(|line| line.to_string())
        .collect();
    }
    Err(_) => {
      return Err(DownloaderError::FetchError);
    }
  }

  let twimg = String::from("https://video.twimg.com");
  let (mut video, mut audio) = (twimg.clone(), twimg.clone());
  match lines.len() > 1 {
    true => {
      let pure_audio = lines[(lines.len() / 2) - 1]
        .split('"')
        .filter(|substring| !substring.is_empty())
        .last()
        .unwrap();
      audio.push_str(pure_audio);
      video.push_str(lines[lines.len() - 1].as_str());

      return Ok((video, audio));
    }
    false => {
      return Err(DownloaderError::MediaPlaylistExtractError);
    }
  }
}

async fn download_segments(urls: (String, String)) -> Result<(Vec<u8>, Vec<u8>), DownloaderError> {
  let video_text = reqwest::get(urls.0)
    .await
    .map_err(|_| DownloaderError::FetchError)?
    .text()
    .await
    .map_err(|_| DownloaderError::FetchError)?;

  let audio_text = reqwest::get(urls.1)
    .await
    .map_err(|_| DownloaderError::FetchError)?
    .text()
    .await
    .map_err(|_| DownloaderError::FetchError)?;

  let video_urls = get_segment_urls(&video_text);
  let audio_urls = get_segment_urls(&audio_text);

  let video_data = Arc::new(Mutex::new(vec![Vec::new(); video_urls.len()]));
  let audio_data = Arc::new(Mutex::new(vec![Vec::new(); audio_urls.len()]));

  let client = Arc::new(
    Client::builder()
      .timeout(Duration::from_secs(10 * 60))
      .build()
      .map_err(|e| DownloaderError::OtherError(e.to_string()))?,
  );

  let video_tasks = get_download_tasks(client.clone(), video_urls, video_data.clone());
  let audio_tasks = get_download_tasks(client.clone(), audio_urls, audio_data.clone());

  let all_tasks: Vec<_> = video_tasks.into_iter().chain(audio_tasks.into_iter()).collect();

  future::join_all(all_tasks).await;

  let video_data = Arc::try_unwrap(video_data).unwrap().into_inner().unwrap().concat();
  let audio_data = Arc::try_unwrap(audio_data).unwrap().into_inner().unwrap().concat();
  Ok((video_data, audio_data))
}

fn get_segment_urls(text: &str) -> Vec<String> {
  text
    .lines()
    .filter(|line| line.contains("/ext_tw_video/") || line.contains("/amplify_video/"))
    .map(|line| {
      let split = line.split('"').filter(|substr| !substr.is_empty());
      let mut result = String::from("https://video.twimg.com");
      if let Some(url) = split.last() {
        result.push_str(url);
      }
      result
    })
    .collect::<Vec<String>>()
}

fn get_download_tasks(client: Arc<Client>, urls: Vec<String>, data: Arc<Mutex<Vec<Vec<u8>>>>) -> Vec<tokio::task::JoinHandle<()>> {
  let mut tasks = Vec::new();
  for (i, url) in urls.into_iter().enumerate() {
    let data = data.clone();
    let client = client.clone();
    let task = tokio::spawn(async move {
      let response = client.get(url).send().await.unwrap();
      let bytes = response.bytes().await.unwrap().to_vec();
      data.lock().unwrap()[i] = bytes;
    });
    tasks.push(task);
  }

  tasks
}
