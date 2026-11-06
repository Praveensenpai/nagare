//! Matching feed items against tracked shows.

use crate::db::Show;
use crate::feed::Item;

/// Normalizes a title for comparison: lowercase, punctuation collapsed to spaces.
#[must_use]
pub fn normalize(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_space = true;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

/// True when the item belongs to the tracked show and is newer than its watermark.
#[must_use]
pub fn matches(show: &Show, item: &Item) -> bool {
    if item.is_batch {
        return false;
    }
    let Some(episode) = item.episode else {
        return false;
    };
    let is_new =
        item.season > show.season || (item.season == show.season && episode > show.max_episode);
    is_new && title_matches(&show.match_title, &item.show)
}

fn title_matches(expected: &str, actual: &str) -> bool {
    let want = normalize(expected);
    let got = normalize(actual);
    if want.is_empty() || got.is_empty() {
        return false;
    }
    want == got || got.starts_with(&want) || want.starts_with(&got)
}

/// Picks the newest acceptable item for a show.
#[must_use]
pub fn pick<'a>(show: &Show, items: &'a [Item]) -> Option<&'a Item> {
    items
        .iter()
        .filter(|item| matches(show, item))
        .max_by(|a, b| {
            let left = (a.season, a.episode.unwrap_or(0.0));
            let right = (b.season, b.episode.unwrap_or(0.0));
            left.0
                .cmp(&right.0)
                .then_with(|| {
                    left.1
                        .partial_cmp(&right.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.released.cmp(&b.released))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn show(season: u32, max: f64) -> Show {
        Show {
            id: 1,
            title: "Show S2".into(),
            match_title: "Show S2".into(),
            season,
            max_episode: max,
            enabled: true,
        }
    }

    fn item(show_name: &str, season: u32, ep: f64) -> Item {
        Item {
            infohash: "H".into(),
            title: format!("[SubsPlease] {show_name} - {ep:02} (1080p).mkv"),
            show: show_name.into(),
            season,
            episode: Some(ep),
            bytes: 1,
            magnet: "magnet:?xt=urn:btih:H".into(),
            released: Utc::now(),
            is_batch: false,
        }
    }

    #[test]
    fn normalize_collapses_punctuation() {
        assert_eq!(normalize("Oshi no Ko!"), "oshi no ko");
        assert_eq!(normalize("  A--B  "), "a b");
    }

    #[test]
    fn matches_only_new_episodes() {
        assert!(matches(&show(2, 4.0), &item("Show S2", 2, 5.0)));
        assert!(!matches(&show(2, 5.0), &item("Show S2", 2, 5.0)));
        assert!(!matches(&show(2, 4.0), &item("Different", 2, 6.0)));
    }

    #[test]
    fn new_season_counts_as_new() {
        assert!(matches(&show(1, 12.0), &item("Show S2", 2, 1.0)));
    }

    #[test]
    fn pick_prefers_highest_episode() {
        let items = vec![item("Show S2", 2, 5.0), item("Show S2", 2, 7.0)];
        assert_eq!(
            pick(&show(2, 4.0), &items).and_then(|i| i.episode),
            Some(7.0)
        );
    }
}
