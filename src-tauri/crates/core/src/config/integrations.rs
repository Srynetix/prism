//! Mail, calendars, Google, Claude sessions and usage, audio.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::*;

/// Sources of the "Audio" block. Without an `[audio]` section, a default selection; a given
/// list replaces its own (stations or feeds, independently).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioConfig {
    #[serde(default = "default_stations")]
    pub stations: Vec<AudioSource>,
    #[serde(default = "default_feeds")]
    pub feeds: Vec<AudioSource>,
    /// YouTube videos, lives or playlists (URL of the page or of the playlist).
    #[serde(default = "default_youtube")]
    pub youtube: Vec<AudioSource>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { stations: default_stations(), feeds: default_feeds(), youtube: default_youtube() }
    }
}

/// A radio (live stream) or a podcast (RSS feed where each episode has its audio file).
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AudioSource {
    pub label: String,
    pub url: String,
}

fn source(label: &str, url: &str) -> AudioSource {
    AudioSource { label: label.into(), url: url.into() }
}

fn default_stations() -> Vec<AudioSource> {
    vec![
        source("FIP", "https://icecast.radiofrance.fr/fip-hifi.aac"),
        source("FIP Groove", "https://icecast.radiofrance.fr/fipgroove-hifi.aac"),
        source("SomaFM · Groove Salad", "https://ice1.somafm.com/groovesalad-128-mp3"),
        source("SomaFM · DEF CON", "https://ice1.somafm.com/defcon-128-mp3"),
        source("Radio Paradise · Mellow", "https://stream.radioparadise.com/mellow-320"),
    ]
}

/// 24/7 lives whose embedding is allowed (checked in October 2026). Channels sometimes
/// restart a live under a new identifier: the old one ends in an error in the player, and
/// `[[audio.youtube]]` allows giving another one.
fn default_youtube() -> Vec<AudioSource> {
    vec![
        source("Lofi Girl · lofi hip hop (study)", "https://www.youtube.com/watch?v=rFZHOHl-L8A"),
        source("Lofi Girl · lofi hip hop (sleep)", "https://www.youtube.com/watch?v=JD-kMIpDfnY"),
        source("Lofi Girl · jazz lofi", "https://www.youtube.com/watch?v=E2vONfzoyRI"),
        source("Lofi Girl · synthwave", "https://www.youtube.com/watch?v=4xDzrJKXOOY"),
        source("Lofi Girl · lofi house", "https://www.youtube.com/watch?v=3PFJ9SETS4M"),
        source("Lofi Girl · asian lofi", "https://www.youtube.com/watch?v=1Tl2FtV06qo"),
        source("Lofi Girl · sad lofi", "https://www.youtube.com/watch?v=CwPCy1GLS38"),
        source("Lofi Girl · summer lofi", "https://www.youtube.com/watch?v=0muHFBSiybw"),
        source("Lofi Girl · sleep lofi", "https://www.youtube.com/watch?v=VAlMDl00mYY"),
        source("Lofi Girl · medieval lofi", "https://www.youtube.com/watch?v=IxPANmjPaek"),
        source("Lofi Girl · chill guitar", "https://www.youtube.com/watch?v=E_XmwjgRLz8"),
        source("Lofi Girl · relaxing piano", "https://www.youtube.com/watch?v=N0snMcR6aaA"),
        source("Lofi Girl · relaxing jazz", "https://www.youtube.com/watch?v=A8jDx9TLMQc"),
        source("Lofi Girl · classical music", "https://www.youtube.com/watch?v=jXAEIWcGXwE"),
        source("Lofi Girl · dark ambient", "https://www.youtube.com/watch?v=S_MOd40zlYU"),
        source("Lofi Girl · synth ambient", "https://www.youtube.com/watch?v=GSfT7H87zq4"),
        source("Lofi Girl · sleep ambient", "https://www.youtube.com/watch?v=xORCbIptqcc"),
        source("Lofi Girl · deep sleep", "https://www.youtube.com/watch?v=nI725iVsyoQ"),
        source("Lofi Girl · study with me (Pomodoro)", "https://www.youtube.com/watch?v=qGohtGC5Rtk"),
        source("Lofi Girl · gentle rain", "https://www.youtube.com/watch?v=-OekvEFm1lo"),
        source("Lofi Girl · fireplace", "https://www.youtube.com/watch?v=q_4KI-ChIIs"),
        source("Lofi Girl · halloween", "https://www.youtube.com/watch?v=1-LpQekNa9g"),
        source("Lofi Girl · christmas", "https://www.youtube.com/watch?v=XSXEaikz0Bc"),
        source("Chillhop · lofi hip hop", "https://www.youtube.com/watch?v=7NOSDKb0HlU"),
        source("Chillhop · jazzy & lofi hip hop", "https://www.youtube.com/watch?v=5yx6BWlEVcY"),
        source("Chillhop · late night vibes", "https://www.youtube.com/watch?v=i6WzngxTnBA"),
        source("Chillhop · essentials", "https://www.youtube.com/watch?v=jiua2V9q9V0"),
        source("Lofi Fruits · lofi hip hop", "https://www.youtube.com/watch?v=TfmECBzmOn4"),
        source("Nightride FM · synthwave", "https://www.youtube.com/watch?v=UjlMEqTu2KI"),
        source("Nightride FM · ChillSynth", "https://www.youtube.com/watch?v=UedTcufyrHc"),
        source("Nightride FM · Datawave", "https://www.youtube.com/watch?v=Y9q6RYg2Pdg"),
        source("Nightride FM · Spacesynth", "https://www.youtube.com/watch?v=wpKPp8hwEbE"),
        source("Radio Cutman · videogame lofi", "https://www.youtube.com/watch?v=lkcjlTqmoDY"),
    ]
}

fn default_feeds() -> Vec<AudioSource> {
    vec![source("musicForProgramming", "https://musicforprogramming.net/rss.xml")]
}

/// Agenda of the day, read from the macOS Calendar or Google Calendar.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalendarConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Where the agenda is read: the macOS Calendar, or Google Calendar (any system).
    #[serde(default)]
    pub provider: CalendarProvider,
    /// The calendars read, by name (case aside); empty: all of them.
    #[serde(default)]
    pub calendars: Vec<String>,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self { enabled: true, provider: CalendarProvider::default(), calendars: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum CalendarProvider {
    /// EventKit: the accounts added to macOS's Calendar, Exchange and Google included.
    #[default]
    Macos,
    /// The Google Calendar API, signed in with `[google]`.
    Google,
}

impl CalendarConfig {
    /// An event of calendar `name` is shown: the calendar is listed, or none is.
    pub fn shows(&self, name: &str) -> bool {
        self.calendars.is_empty() || self.calendars.iter().any(|c| c.trim().eq_ignore_ascii_case(name))
    }
}

/// What the Claude Desktop sessions' collector reads: `[agents.claude]`'s keys (`ClaudeConfig::sessions`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionsConfig {
    /// Metadata of the desktop's Claude Code sessions.
    #[serde(default = "default_sessions_dir")]
    pub dir: PathBuf,
    /// Claude Code transcripts, to tell whether a session is working.
    #[serde(default = "default_transcripts_dir")]
    pub transcripts: PathBuf,
    #[serde(default = "default_sessions_poll")]
    pub poll_seconds: u64,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self { dir: default_sessions_dir(), transcripts: default_transcripts_dir(), poll_seconds: default_sessions_poll() }
    }
}

fn default_sessions_dir() -> PathBuf {
    crate::platform::claude_desktop_dir().join("claude-code-sessions")
}

fn default_transcripts_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude/projects")
}

fn default_sessions_poll() -> u64 {
    3
}

/// The Mail block (`[mail]`): the unread messages of the Microsoft 365 inbox, through Microsoft
/// Graph with the delegated `Mail.ReadBasic` permission (your mailbox only, no body), or of the
/// Gmail inbox with `gmail.metadata` (headers and labels only).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailConfig {
    /// Off by default: on, you sign in once from Settings.
    #[serde(default)]
    pub enabled: bool,
    /// The inbox read: Microsoft 365 (Graph), or Gmail (signed in with `[google]`).
    #[serde(default)]
    pub provider: MailProvider,
    /// The application registered in Entra ID that Prism signs in with (its "Application (client)
    /// ID"), allowed the delegated Mail.ReadBasic permission. A team's: in its pack.
    #[serde(default)]
    pub client_id: String,
    /// The Entra ID tenant (its id or domain). Empty: `organizations`, any work account.
    #[serde(default)]
    pub tenant: String,
    /// The flagged messages too, read or not.
    #[serde(default = "default_true")]
    pub flagged: bool,
    /// Messages listed at most, newest first.
    #[serde(default = "default_mail_max")]
    pub max: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum MailProvider {
    #[default]
    Microsoft,
    Gmail,
}

/// Your Google Cloud project's OAuth client, of the "Desktop app" type. Its secret goes into the
/// system keychain, never here.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GoogleConfig {
    #[serde(default)]
    pub client_id: String,
}

impl Default for MailConfig {
    fn default() -> Self {
        toml::from_str("").expect("MailConfig defaults")
    }
}

fn default_mail_max() -> u32 {
    30
}
