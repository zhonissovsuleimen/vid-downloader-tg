use futures::future::join_all;
use regex::Regex;
use std::collections::HashMap;

use crate::downloader::{downloader_error::DownloaderError, playlist::master_playlist::MasterPlaylist};

pub struct VariantPlaylist {
  pub master_playlists: Vec<MasterPlaylist>,
}

impl VariantPlaylist {
  pub async fn from_url(url: &str) -> Result<Self, DownloaderError> {
    const BASE_URL: &str = "https://video.twimg.com";

    let response =
      reqwest::get(url).await.map_err(|_| DownloaderError::FetchError)?.text().await.map_err(|_| DownloaderError::FetchError)?;

    //audio pass
    const AUDIO_REGEX_STR: &str = r#"(?m)^#EXT-X-MEDIA:NAME="Audio".*GROUP-ID="(?P<audio_tag>audio-\d*)".*URI="(?P<audio_url>.*)""#;

    let mut audio_map = HashMap::<String, String>::new();
    let audio_pass = Regex::new(&AUDIO_REGEX_STR).unwrap();
    for capture in audio_pass.captures_iter(&response) {
      let Some(tag) = capture.name("audio_tag") else {
        continue;
      };
      let Some(url) = capture.name("audio_url") else {
        continue;
      };
      audio_map.insert(tag.as_str().to_string(), url.as_str().to_string());
    }

    //video pass
    let contains_audio = audio_map.len() > 0;
    let video_regex_str: &str = if contains_audio {
      r#"(?m)^#EXT-X-STREAM-INF.*RESOLUTION=(?P<video_resolution>\d*x\d*).*AUDIO="(?P<audio_match_tag>audio-\d*)"\n(?P<video_url>.*)"#
    } else {
      r#"(?m)^#EXT-X-STREAM-INF.*RESOLUTION=(?P<video_resolution>\d*x\d*).*\n(?P<video_url>.*)"#
    };

    let mut tasks = vec![];
    let video_pass = Regex::new(video_regex_str).unwrap();
    for capture in video_pass.captures_iter(&response) {
      let Some(resolution) = capture.name("video_resolution") else {
        continue;
      };
      let Some(video_url) = capture.name("video_url") else {
        continue;
      };
      let audio_url = if contains_audio {
        capture.name("audio_match_tag").and_then(|tag| audio_map.get(tag.as_str()).cloned())
      } else {
        None
      };

      let full_video_url = format!("{BASE_URL}{}", video_url.as_str());
      let full_audio_url = audio_url.map(|url| format!("{BASE_URL}{}", url));
      let resolution_string = resolution.as_str().to_string();

      tasks.push(tokio::spawn(async move {
        match MasterPlaylist::from_urls(full_video_url, full_audio_url).await {
          Ok(mut master_playlist) => {
            master_playlist.resolution = resolution_string;
            Ok(master_playlist)
          }
          Err(e) => {
            println!("Error: {}", e);
            Err(())
          }
        }
      }));
    }

    let results = join_all(tasks).await;
    let mut master_playlists: Vec<MasterPlaylist> = vec![];

    for result in results {
      if let Ok(Ok(master_playlist)) = result {
        master_playlists.push(master_playlist);
      }
    }

    //sorting by resolution descending
    master_playlists.sort_by(|a, b| {
      let a_product: i32 = a.resolution.split('x').map(|s| s.parse::<i32>().unwrap()).product();
      let b_product: i32 = b.resolution.split('x').map(|s| s.parse::<i32>().unwrap()).product();
      b_product.cmp(&a_product)
    });

    Ok(VariantPlaylist { master_playlists: master_playlists })
  }
}
