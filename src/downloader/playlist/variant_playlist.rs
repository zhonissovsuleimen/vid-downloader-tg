use futures::future::join_all;
use regex::Regex;
use std::collections::HashMap;

use super::master_playlist::MasterPlaylist;
use crate::downloader::downloader_error::DownloaderError;

pub struct VariantPlaylist {
  pub master_playlists: Vec<MasterPlaylist>,
}

impl VariantPlaylist {
  pub async fn from_url(url: &str) -> Result<Self, DownloaderError> {
    const BASE_URL: &str = "https://video.twimg.com";
    struct ParsedOutput {
      video_url: Option<String>,
      audio_url: Option<String>,
      resolution: Option<String>,
    }

    let mut output_map = HashMap::<String, ParsedOutput>::new();

    let response =
      reqwest::get(url).await.map_err(|_| DownloaderError::FetchError)?.text().await.map_err(|_| DownloaderError::FetchError)?;

    let lines = response.lines().filter(|&line| !line.is_empty()).collect::<Vec<&str>>();

    const AUDIO_REGEX_STR: &str = r#"^#EXT-X-MEDIA:NAME="Audio".*GROUP-ID="(?P<audio_tag>.*)".*URI="(?P<audio_media_url>.*)"$"#;
    const VIDEO_REGEX_STR: &str = r#"^#EXT-X-STREAM-INF.*RESOLUTION=(?P<video_resolution>\d*x\d*),.*AUDIO="(?P<audio_match_tag>.*)"$"#;
    let final_regex_string = format!("{AUDIO_REGEX_STR}|{VIDEO_REGEX_STR}");

    let regex = Regex::new(&final_regex_string).unwrap();
    for (i, line) in lines.iter().enumerate() {
      match regex.captures(line) {
        Some(audio_captures) if audio_captures.name("audio_media_url").is_some() => {
          let mut audio_media_url = audio_captures.name("audio_media_url").unwrap().as_str().to_string();
          audio_media_url = format!("{BASE_URL}{audio_media_url}");

          if let Some(audio_tag) = audio_captures.name("audio_tag").map(|tag| tag.as_str().to_string()) {
            output_map.entry(audio_tag).and_modify(|o| o.audio_url = Some(audio_media_url.clone())).or_insert(ParsedOutput {
              video_url: None,
              audio_url: Some(audio_media_url.clone()),
              resolution: None,
            });
          }
        }
        Some(video_captures) if video_captures.name("audio_match_tag").is_some() => {
          let audio_match_tag = video_captures.name("audio_match_tag").unwrap().as_str().to_string();
          let mut video_media_url = lines[i + 1].to_string();
          video_media_url = format!("{BASE_URL}{video_media_url}");

          if let Some(resolution) = video_captures.name("video_resolution").map(|res| res.as_str().to_string()) {
            output_map
              .entry(audio_match_tag)
              .and_modify(|o| {
                o.video_url = Some(video_media_url.clone());
                o.resolution = Some(resolution.clone());
              })
              .or_insert(ParsedOutput {
                video_url: Some(video_media_url.clone()),
                audio_url: None,
                resolution: Some(resolution.clone()),
              });
          }
        }
        _ => {}
      }
    }

    let mut tasks = vec![];
    for output in output_map.values() {
      if output.video_url.is_none() || output.audio_url.is_none() || output.resolution.is_none() {
        continue;
      }

      let video_url = output.video_url.clone().unwrap();
      let audio_url = output.audio_url.clone().unwrap();
      let resolution = output.resolution.clone().unwrap();
      tasks.push(tokio::spawn(async move {
        match MasterPlaylist::from_urls(&video_url, &audio_url).await {
          Ok(mut master_playlist) => {
            master_playlist.resolution = resolution;
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
