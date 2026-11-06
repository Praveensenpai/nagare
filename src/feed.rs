//! SubsPlease RSS parsing and conditional fetching.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::time::Duration;

/// One release item from the feed.
#[derive(Debug, Clone)]
pub struct Item {
    /// Base32 infohash from the guid element.
    pub infohash: String,
    /// Raw release title.
    pub title: String,
    /// Show name derived from the category or title.
    pub show: String,
    /// Season parsed from an `S<n>` marker, otherwise 1.
    pub season: u32,
    /// Episode number within the season.
    pub episode: Option<f64>,
    /// Exact size in bytes, from the magnet `xl=` when present.
    pub bytes: u64,
    /// Full magnet URI.
    pub magnet: String,
    /// Publication timestamp.
    pub released: DateTime<Utc>,
    /// True for multi-episode batch releases.
    pub is_batch: bool,
}

impl Item {
    /// Human label such as `S02E05`.
    #[must_use]
    pub fn episode_label(&self) -> String {
        let ep = self.episode.unwrap_or(0.0);
        format!("S{:02}E{:02}", self.season, ep as u32)
    }
}

/// Result of one conditional feed fetch.
#[derive(Debug, Default)]
pub struct FetchOutcome {
    /// Parsed items, empty when the server answered 304.
    pub items: Vec<Item>,
    /// True when the feed was unchanged.
    pub not_modified: bool,
    /// ETag validator to send next time.
    pub etag: Option<String>,
    /// Last-Modified validator to send next time.
    pub last_modified: Option<String>,
}

/// Fetches the feed, sending stored validators so unchanged feeds cost a 304.
///
/// # Errors
/// Returns an error when the request fails, the status is not success, or the
/// XML cannot be parsed.
pub async fn fetch(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<FetchOutcome> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("nagare/0.1")
        .build()
        .context("failed to build HTTP client")?;
    let mut request = client.get(url);
    if let Some(value) = etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, value);
    }
    if let Some(value) = last_modified {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, value);
    }
    let response = request.send().await.context("failed to fetch RSS feed")?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(FetchOutcome {
            not_modified: true,
            ..FetchOutcome::default()
        });
    }
    let response = response
        .error_for_status()
        .context("RSS endpoint returned an error")?;
    let etag = header_string(response.headers(), reqwest::header::ETAG);
    let last_modified = header_string(response.headers(), reqwest::header::LAST_MODIFIED);
    let body = response.text().await.context("failed to read RSS body")?;
    Ok(FetchOutcome {
        items: parse(&body)?,
        not_modified: false,
        etag,
        last_modified,
    })
}

fn header_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}

/// Parses an RSS document into release items.
///
/// # Errors
/// Returns an error when the XML is malformed.
pub fn parse(body: &str) -> Result<Vec<Item>> {
    let doc = roxmltree::Document::parse(body).context("failed to parse RSS XML")?;
    let items = doc
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|node| parse_item(&node))
        .collect();
    Ok(items)
}

fn parse_item(node: &roxmltree::Node<'_, '_>) -> Option<Item> {
    let title = child_text(node, "title")?;
    let magnet = child_text(node, "link")?;
    if !magnet.starts_with("magnet:") {
        return None;
    }
    let infohash = child_text(node, "guid")?;
    let category = child_text(node, "category").unwrap_or_default();
    let released = child_text(node, "pubDate")
        .and_then(|value| DateTime::parse_from_rfc2822(&value).ok())
        .map_or_else(Utc::now, |value| value.with_timezone(&Utc));
    let (season, episode, is_batch) = parse_numbering(&title);
    let show = show_from_category(&category).unwrap_or_else(|| show_from_title(&title));
    let bytes = magnet_size(&magnet)
        .or_else(|| child_text(node, "size").map(|value| parse_size(&value)))
        .unwrap_or(0);
    Some(Item {
        infohash,
        title,
        show,
        season,
        episode,
        bytes,
        magnet,
        released,
        is_batch,
    })
}

fn child_text(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn parse_numbering(title: &str) -> (u32, Option<f64>, bool) {
    let stem = title.strip_suffix(".mkv").unwrap_or(title);
    let is_batch = stem.to_lowercase().contains("batch");
    let Some(index) = stem.rfind(" - ") else {
        return (1, None, is_batch);
    };
    let before = &stem[..index];
    let after = &stem[index + 3..];
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    let episode = digits.parse::<f64>().ok();
    let season = before
        .rsplit(' ')
        .next()
        .and_then(|token| token.strip_prefix('S').or_else(|| token.strip_prefix('s')))
        .and_then(|number| number.parse::<u32>().ok())
        .unwrap_or(1);
    (season, episode, is_batch)
}

fn show_from_category(category: &str) -> Option<String> {
    let (name, tail) = category.rsplit_once(" - ")?;
    let quality = tail.trim();
    if quality.is_empty() || !quality.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(name.trim().to_string())
}

fn show_from_title(title: &str) -> String {
    let stem = title.strip_suffix(".mkv").unwrap_or(title);
    let without_prefix = stem.strip_prefix("[SubsPlease] ").unwrap_or(stem);
    match without_prefix.rfind(" - ") {
        Some(index) => without_prefix[..index].trim().to_string(),
        None => without_prefix.trim().to_string(),
    }
}

fn magnet_size(magnet: &str) -> Option<u64> {
    let index = magnet.find("xl=")?;
    let rest = &magnet[index + 3..];
    let end = rest.find('&').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn parse_size(value: &str) -> u64 {
    let mut parts = value.split_whitespace();
    let number = parts
        .next()
        .and_then(|token| token.parse::<f64>().ok())
        .unwrap_or(0.0);
    let multiplier = match parts.next().unwrap_or("B") {
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (number * multiplier) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:subsplease="https://subsplease.org/rss">
  <channel>
    <item>
      <title>[SubsPlease] Frieren S2 - 05 (1080p) [ABCD1234].mkv</title>
      <link>magnet:?xt=urn:btih:ABCD1234&amp;dn=Frieren&amp;xl=1480803613</link>
      <guid isPermaLink="false">ABCD1234</guid>
      <pubDate>Thu, 05 Nov 2026 09:02:02 +0000</pubDate>
      <category>Frieren S2 - 1080</category>
    </item>
    <item>
      <title>[SubsPlease] Show (01-12) (1080p) [BATCH].mkv</title>
      <link>magnet:?xt=urn:btih:BATCH</link>
      <guid>BATCH</guid>
      <category>Show - 1080</category>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parses_show_season_and_episode() {
        let items = parse(SAMPLE).expect("parse");
        assert_eq!(items.len(), 2);
        let first = &items[0];
        assert_eq!(first.show, "Frieren S2");
        assert_eq!(first.season, 2);
        assert_eq!(first.episode, Some(5.0));
        assert_eq!(first.bytes, 1_480_803_613);
        assert_eq!(first.episode_label(), "S02E05");
    }

    #[test]
    fn marks_batches() {
        let items = parse(SAMPLE).expect("parse");
        assert!(items[1].is_batch);
    }

    #[test]
    fn parses_human_sizes() {
        assert_eq!(parse_size("1.38 GiB"), 1_481_763_717);
    }

    #[test]
    fn season_defaults_to_one() {
        let (season, episode, _) = parse_numbering("[SubsPlease] One Piece - 1089 (1080p).mkv");
        assert_eq!(season, 1);
        assert_eq!(episode, Some(1089.0));
    }
}
