//! "Audio" block of the status bar: playback happens in the WebView; the Rust core provides
//! the sources from the config, reads the podcasts' RSS feeds (no CORS on the Rust side) and relays
//! the system's "Now Playing" (`now_playing`, macOS and Linux).

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;

use crate::config::{AudioConfig, AudioSource};

pub mod now_playing;

#[derive(Debug, Clone, Serialize)]
pub struct Sources {
    pub stations: Vec<AudioSource>,
    pub feeds: Vec<AudioSource>,
    pub youtube: Vec<AudioSource>,
}

impl From<&AudioConfig> for Sources {
    fn from(cfg: &AudioConfig) -> Self {
        Self { stations: cfg.stations.clone(), feeds: cfg.feeds.clone(), youtube: cfg.youtube.clone() }
    }
}

/// A podcast episode, from most recent to oldest.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Episode {
    pub title: String,
    pub url: String,
    /// The feed's `pubDate`, as is (RFC 2822).
    pub published: Option<String>,
    /// `itunes:duration` in seconds, if the feed provides it.
    pub duration_s: Option<u64>,
}

pub async fn episodes(feed: &AudioSource) -> Result<Vec<Episode>> {
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build()?;
    let xml = http.get(&feed.url).send().await?.error_for_status()?.text().await.with_context(|| format!("flux {}", feed.label))?;
    Ok(parse(&xml))
}

/// Minimal reading of a podcast RSS: title, file (`enclosure`), date, duration.
fn parse(xml: &str) -> Vec<Episode> {
    let item = Regex::new(r"(?s)<item>(.*?)</item>").unwrap();
    let title = Regex::new(r"(?s)<title>(.*?)</title>").unwrap();
    let enclosure = Regex::new(r#"<enclosure[^>]*\burl="([^"]+)""#).unwrap();
    let date = Regex::new(r"<pubDate>(.*?)</pubDate>").unwrap();
    let duration = Regex::new(r"<itunes:duration>(.*?)</itunes:duration>").unwrap();
    item.captures_iter(xml)
        .filter_map(|c| {
            let body = &c[1];
            Some(Episode {
                title: title.captures(body).map(|t| text(&t[1])).unwrap_or_default(),
                url: decode(&enclosure.captures(body)?[1]),
                published: date.captures(body).map(|d| d[1].trim().to_string()),
                duration_s: duration.captures(body).and_then(|d| seconds(d[1].trim())),
            })
        })
        .collect()
}

fn text(raw: &str) -> String {
    let raw = raw.trim();
    let raw = raw.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")).unwrap_or(raw);
    decode(raw.trim())
}

fn decode(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&amp;", "&")
}

/// `4:00:00`, `52:10` or `3130`.
fn seconds(s: &str) -> Option<u64> {
    s.split(':').try_fold(0u64, |acc, part| Some(acc * 60 + part.parse::<u64>().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_podcast_items() {
        let xml = r#"<rss><channel><title>Music For Programming</title>
            <item><title>Episode 79: Corticyte</title><pubDate>Mon, 24 Aug 2026 17:18:00 GMT</pubDate>
            <itunes:duration>4:00:00</itunes:duration>
            <enclosure url="https://datashat.net/music_for_programming_79-corticyte.mp3" length="1" type="audio/mpeg"/></item>
            <item><title><![CDATA[Rock &amp; Roll]]></title><enclosure type="audio/mpeg" url="https://x/a.mp3?a=1&amp;b=2"/></item>
            <item><title>No file</title></item>
        </channel></rss>"#;
        assert_eq!(
            parse(xml),
            vec![
                Episode {
                    title: "Episode 79: Corticyte".into(),
                    url: "https://datashat.net/music_for_programming_79-corticyte.mp3".into(),
                    published: Some("Mon, 24 Aug 2026 17:18:00 GMT".into()),
                    duration_s: Some(14_400),
                },
                Episode { title: "Rock & Roll".into(), url: "https://x/a.mp3?a=1&b=2".into(), published: None, duration_s: None },
            ]
        );
    }

    #[test]
    fn durations() {
        assert_eq!(seconds("52:10"), Some(3130));
        assert_eq!(seconds("3130"), Some(3130));
        assert_eq!(seconds("n/a"), None);
    }
}
