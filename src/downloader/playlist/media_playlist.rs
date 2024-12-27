use regex::Regex;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::downloader::downloader_error::DownloaderError;

pub struct MediaPlaylist {
  pub name: String,
  byte_data: Vec<u8>,
}

impl MediaPlaylist {
  pub async fn from_url(url: &str) -> Result<Self, DownloaderError> {
    info!("Fetching media playlist from: {}", url);
    let mut name = String::new();
    const BASE_URL: &str = "https://video.twimg.com";

    let response =
      reqwest::get(url).await.map_err(|_| DownloaderError::FetchError)?.text().await.map_err(|_| DownloaderError::FetchError)?;
    let lines = response.lines().filter(|&line| !line.is_empty()).collect::<Vec<&str>>();

    const BASE_SEGMENT_REGEX_STR: &str = r#"#EXT-X-MAP:URI="(?P<base_segment_url>.*)""#;
    const OTHER_SEGMENTS_REGEX_STR: &str = r#"^(?P<segment_url>\/.*)$"#;
    let final_regex_string = format!("{BASE_SEGMENT_REGEX_STR}|{OTHER_SEGMENTS_REGEX_STR}");

    let regex = Regex::new(&final_regex_string).unwrap();
    let mut ordered_urls = Vec::<String>::new();
    for line in lines {
      match regex.captures(line) {
        Some(base_captures) if base_captures.name("base_segment_url").is_some() => {
          let url = base_captures.name("base_segment_url").unwrap().as_str().to_string();
          name = url.to_string();
          ordered_urls.push(format!("{BASE_URL}{url}"));
        }
        Some(other_captures) if other_captures.name("segment_url").is_some() => {
          let url = other_captures.name("segment_url").unwrap().as_str().to_string();
          ordered_urls.push(format!("{BASE_URL}{url}"));
        }
        _ => {}
      }
    }

    let ordered_bytes = Arc::new(Mutex::new(vec![vec![]; ordered_urls.len()]));
    let mut tasks = vec![];
    for (i, url) in ordered_urls.iter().enumerate() {
      let result_clone = ordered_bytes.clone();
      let url_clone = url.clone();
      tasks.push(tokio::spawn(async move {
        let client = reqwest::Client::new();
        if let Ok(response) = client.get(url_clone).send().await {
          if let Ok(bytes) = response.bytes().await {
            result_clone.lock().unwrap()[i] = bytes.to_vec();
          }
        };
      }));
    }

    let _ = futures::future::join_all(tasks).await;
    let bytes_data = ordered_bytes.lock().unwrap().iter().flatten().cloned().collect::<Vec<u8>>();

    Ok(MediaPlaylist { name: name, byte_data: bytes_data })
  }

  pub fn get_byte_data(&self) -> &Vec<u8> {
    &self.byte_data
  }
}
