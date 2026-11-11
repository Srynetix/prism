use prism_base::{i18n::tr, tr};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{
    agents::provider::Provider,
    state::{ClaudeUsage, Hub, UsageWindow},
};

const SOURCE: &str = "usage";
/// Usage moves slowly on the scale of a 5 h window, and the endpoint rate-limits quickly (429):
/// one reading every 5 minutes is enough (far less often with the window hidden).
const POLL: Duration = Duration::from_secs(300);
/// After a 429 without `Retry-After`: the wait doubles on each refusal, up to an hour.
const BACKOFF_MAX: Duration = Duration::from_secs(3600);
const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
/// Keychain entry where Claude Code stores its OAuth credentials (macOS), a suffix after it for a
/// configuration folder of its own (`CLAUDE_CONFIG_DIR`); elsewhere, a file of that folder.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Plan usage of the agents' CLI (`[agents] provider`): 5 h window and week, as `/usage` shows
/// it in Claude Code or Command Code (see `command_code`), or Codex (see `codex`); OpenCode, which
/// runs on the user's own providers, has none. A change of provider is followed at once. The other
/// CLIs agents run on meanwhile have theirs read too (see `others`).
///
/// Claude Code's OAuth token is read from the keychain on each request and never leaves
/// this function: neither log nor snapshot. Prism does not renew it itself, so as not to
/// invalidate Claude Code's: when expired (or about to be) or refused, it runs
/// `claude doctor`, which renews it without calling the model (`[agents.claude] refresh_token`,
/// with its `bin`), then retries immediately; at most once per `REFRESH_EVERY`.
pub async fn run(hub: Hub) {
    prism_base::rt::spawn(others(hub.clone()));
    let http = crate::http::client();
    let mut meter: Option<Meter> = None;
    let mut wait = Duration::ZERO;
    loop {
        let mut settings = hub.subscribe();
        // `[agents] provider` under its default account (`[[agents.accounts]]`), each its own plan.
        let current = Plan::configured(&settings.borrow().agents_settings);
        if meter.as_ref().is_none_or(|m| m.plan != current) {
            let (m, cached, due) = Meter::start(current.clone());
            meter = Some(m);
            wait = due;
            let account = current.account.clone();
            hub.update(|s| {
                let changed = s.usage != cached || s.usage_account != account;
                s.usage = cached;
                s.usage_account = account;
                changed
            });
            hub.set_error(SOURCE, None);
        }
        let Some(m) = meter.as_mut() else { continue };
        let moved = |s: &crate::state::Snapshot| Plan::configured(&s.agents_settings) != current;
        if !wait.is_zero() {
            tokio::select! {
                _ = hub.pause(wait) => {}
                _ = async { let _ = settings.wait_for(moved).await; } => continue,
            }
        }
        // No plan to read (and no cached reading to wait for): nothing until another provider is chosen.
        if !current.provider.has_plan() {
            if settings.wait_for(moved).await.is_err() {
                return;
            }
            continue;
        }
        let claude = settings.borrow().agents_settings.claude.clone();
        let (result, next) = m.read(&http, &claude).await;
        wait = next;
        match result {
            Ok(usage) => {
                hub.update(|s| {
                    let changed = s.usage.as_ref() != Some(&usage);
                    s.usage = Some(usage);
                    changed
                });
                hub.set_error(SOURCE, None);
            }
            Err(e) => hub.set_error(SOURCE, Some(e)),
        }
    }
}

/// A plan read: a CLI's, under one of its accounts (`[[agents.accounts]]`) or its usual folder.
#[derive(Debug, Clone, PartialEq)]
struct Plan {
    provider: Provider,
    account: Option<String>,
    /// That account's folder; none: the CLI's usual one, as before accounts.
    home: Option<PathBuf>,
}

impl Plan {
    /// `provider` under account `name` (none: its usual folder); an account no longer declared reads as none.
    fn of(cfg: &crate::config::AgentsConfig, provider: Provider, name: Option<&str>) -> Self {
        let account = name.and_then(|n| cfg.account(provider, Some(n)));
        // Its usual folder: read as before (see `Provider::own_home`).
        Self { provider, account: account.map(|a| a.name.clone()), home: account.and_then(|a| provider.own_home(&a.dir)) }
    }

    /// `[agents] provider` under its default account.
    fn configured(cfg: &crate::config::AgentsConfig) -> Self {
        let name = cfg.account(cfg.provider, None).map(|a| a.name.clone());
        Self::of(cfg, cfg.provider, name.as_deref())
    }
}

/// The plans (a CLI with a plan, under an account) that agents run on, other than
/// `[agents] provider`'s default one, in the order their first running agent was launched.
fn active_others(s: &crate::state::Snapshot) -> Vec<Plan> {
    let cfg = &s.agents_settings;
    let configured = Plan::configured(cfg);
    let mut running: Vec<&crate::state::Agent> = s.agents.iter().filter(|a| a.state == crate::state::AgentState::Running).collect();
    running.sort_by_key(|a| a.created_at);
    let mut out: Vec<Plan> = Vec::new();
    for a in running {
        let plan = Plan::of(cfg, a.provider, a.account.as_deref());
        if plan != configured && a.provider.has_plan() && !out.contains(&plan) {
            out.push(plan);
        }
    }
    out
}

/// The plan usage of the other CLIs agents run on (`other_usage`): each read at its own pace, from
/// the same cache, as long as one of its agents runs; dropped once none does.
async fn others(hub: Hub) {
    let http = crate::http::client();
    // Each CLI's meter, and when it is next due.
    let mut meters: Vec<(Meter, tokio::time::Instant)> = Vec::new();
    let mut settings = hub.subscribe();
    loop {
        let active = active_others(&settings.borrow_and_update());
        if active.iter().ne(meters.iter().map(|(m, _)| &m.plan)) {
            let mut kept: Vec<(Meter, tokio::time::Instant)> = Vec::new();
            let mut entries: Vec<crate::state::ProviderUsage> = Vec::new();
            for p in &active {
                let old = hub.subscribe().borrow().other_usage.iter().find(|u| u.provider == p.provider && u.account == p.account).cloned();
                match meters.iter().position(|(m, _)| m.plan == *p) {
                    Some(i) => kept.push(meters.swap_remove(i)),
                    None => {
                        let (m, cached, due) = Meter::start(p.clone());
                        kept.push((m, tokio::time::Instant::now() + due));
                        entries.push(crate::state::ProviderUsage { provider: p.provider, account: p.account.clone(), usage: cached, error: None });
                        continue;
                    }
                }
                entries.push(old.unwrap_or(crate::state::ProviderUsage { provider: p.provider, account: p.account.clone(), usage: None, error: None }));
            }
            meters = kept;
            hub.update(|s| {
                let changed = s.other_usage != entries;
                s.other_usage = entries;
                changed
            });
        }
        let next = meters.iter().map(|(_, at)| *at).min();
        tokio::select! {
            changed = settings.changed() => {
                if changed.is_err() {
                    return;
                }
                continue;
            }
            _ = async {
                match next {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {}
        }
        let claude = hub.subscribe().borrow().agents_settings.claude.clone();
        let now = tokio::time::Instant::now();
        for (m, at) in meters.iter_mut().filter(|(_, at)| *at <= now) {
            let (result, wait) = m.read(&http, &claude).await;
            // Read less often with the window hidden, as the configured CLI's (`Hub::pause`).
            *at = tokio::time::Instant::now() + if hub.shown() { wait } else { (wait * 4).max(Duration::from_secs(60)) };
            let plan = m.plan.clone();
            hub.update(|s| {
                let Some(u) = s.other_usage.iter_mut().find(|u| u.provider == plan.provider && u.account == plan.account) else { return false };
                let before = u.clone();
                match result {
                    Ok(usage) => {
                        u.usage = Some(usage);
                        u.error = None;
                    }
                    Err(e) => u.error = Some(e),
                }
                *u != before
            });
        }
    }
}

/// A plan's reader: its back-off after refusals, and Claude Code's token renewals.
struct Meter {
    plan: Plan,
    backoff: Duration,
    refreshed: Option<std::time::Instant>,
}

impl Meter {
    /// A reader for `provider`, its last reading on disk and how long until the next is due: an
    /// app restart (frequent during development) shows it again without calling the endpoint as
    /// long as it is younger than POLL.
    fn start(plan: Plan) -> (Self, Option<ClaudeUsage>, Duration) {
        let cached = cache::load(plan.provider, plan.account.as_deref());
        let due = cached.as_ref().map_or(Duration::ZERO, |u| POLL.saturating_sub((Utc::now() - u.fetched_at).to_std().unwrap_or_default()));
        (Self { plan, backoff: POLL, refreshed: None }, cached, due)
    }

    /// One reading, cached on success, and the wait before the next; the error as shown.
    async fn read(&mut self, http: &reqwest::Client, claude: &crate::config::ClaudeConfig) -> (Result<ClaudeUsage, String>, Duration) {
        let home = self.plan.home.clone();
        let mut result = match self.plan.provider {
            Provider::Claude => fetch(http, home.as_deref()).await,
            Provider::CommandCode => command_code::fetch(http).await,
            Provider::Codex => codex::fetch(home.clone()).await,
            Provider::OpenCode => return (Err(tr("OpenCode has no plan").into()), BACKOFF_MAX),
        };
        if let Err(expired) = &result
            && self.plan.provider == Provider::Claude
            && expired.is::<Expired>()
            && claude.refresh_token
            && self.refreshed.is_none_or(|t| t.elapsed() >= REFRESH_EVERY)
        {
            self.refreshed = Some(std::time::Instant::now());
            // Shown with the error: without it, a renewal that fails or changes nothing goes unseen.
            result = match refresh(claude.cli.bin.as_deref().unwrap_or("claude"), home.as_deref()).await {
                Ok(()) => fetch(http, home.as_deref()).await.map_err(|e| if e.is::<Expired>() { anyhow::anyhow!(tr!("{err} (claude doctor did not renew it)", err = format!("{e:#}"))) } else { e }),
                Err(e) => {
                    tracing::warn!("usage: claude doctor: {e:#}");
                    Err(anyhow::anyhow!(tr!("{err} (claude doctor failed: {why})", err = format!("{expired:#}"), why = format!("{e:#}"))))
                }
            };
        }
        match result {
            Ok((usage, body)) => {
                cache::save(self.plan.provider, self.plan.account.as_deref(), &body);
                self.backoff = POLL;
                (Ok(usage), POLL)
            }
            Err(e) => {
                // The last reading stays displayed; only the message reports the refusal.
                let wait = match e.downcast_ref::<RateLimited>() {
                    Some(RateLimited(Some(after))) => (*after).max(POLL),
                    Some(RateLimited(None)) => {
                        self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
                        self.backoff
                    }
                    None => POLL,
                };
                let retry = Utc::now() + chrono::Duration::from_std(wait).unwrap_or_default();
                tracing::warn!("usage ({:?}{}): {e:#}, next try at {retry}", self.plan.provider, self.plan.account.as_deref().map(|a| format!(", {a}")).unwrap_or_default());
                (Err(tr!("{err} · next try {at}", err = format!("{e:#}"), at = retry.with_timezone(&chrono::Local).format("%H:%M"))), wait)
            }
        }
    }
}

/// Claude Code token expired, about to expire, or refused.
#[derive(Debug)]
struct Expired(&'static str);

impl std::fmt::Display for Expired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Claude Code token {}: open Claude Code once to refresh it", self.0)
    }
}

impl std::error::Error for Expired {}

/// At most one renewal per quarter hour, even if it fails.
const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);
/// Token expiring in less than 5 minutes: renewed beforehand.
const EXPIRY_MARGIN: chrono::TimeDelta = chrono::TimeDelta::minutes(5);

/// `claude doctor` renews Claude Code's OAuth token along the way, without calling the model.
/// Run outside any project (Prism's cache folder), without a terminal, one minute at most.
async fn refresh(claude: &str, home: Option<&Path>) -> Result<()> {
    let dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("prism");
    std::fs::create_dir_all(&dir).ok();
    let mut command = crate::platform::command(claude);
    // An account's: its own login renewed.
    if let Some(home) = home {
        command.env("CLAUDE_CONFIG_DIR", home);
    }
    let mut child = command
        .arg("doctor")
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| tr!("{cli} not found", cli = claude))?;
    match tokio::time::timeout(Duration::from_secs(60), child.wait()).await {
        Ok(status) => {
            status?;
            Ok(())
        }
        Err(_) => bail!("{}", tr("timed out")),
    }
}

/// A 429 refusal from the endpoint, with the delay requested by `Retry-After` if there is one.
#[derive(Debug)]
struct RateLimited(Option<Duration>);

impl std::fmt::Display for RateLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "usage endpoint rate-limited (HTTP 429)")
    }
}

impl std::error::Error for RateLimited {}

async fn fetch(http: &reqwest::Client, home: Option<&Path>) -> Result<(ClaudeUsage, serde_json::Value)> {
    let token = access_token(home).await?;
    let resp = http
        .get(ENDPOINT)
        .bearer_auth(&token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Expired("rejected").into());
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        return Err(RateLimited(after).into());
    }
    if !status.is_success() {
        bail!("usage endpoint: HTTP {status}");
    }
    let body: serde_json::Value = resp.json().await.context("unreadable usage response")?;
    Ok((parse(&body), body))
}

/// Disk cache of the last response (percentages and reset dates, nothing else).
mod cache {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    use crate::{agents::provider::Provider, state::ClaudeUsage};

    #[derive(Serialize, Deserialize)]
    struct Saved {
        /// Absent: Claude Code, the only provider before the choice existed.
        #[serde(default)]
        provider: Provider,
        fetched_at: DateTime<Utc>,
        body: serde_json::Value,
    }

    /// One file per CLI and account: Claude Code's usual one keeps the name it had when it was
    /// the only one; an account's adds its name (its letters and digits).
    fn path(provider: Provider, account: Option<&str>) -> Option<PathBuf> {
        let mut name = match provider {
            Provider::Claude => "claude-usage".to_string(),
            p => format!("usage-{}", serde_json::to_value(p).ok()?.as_str()?),
        };
        if let Some(a) = account {
            name.push('-');
            name.extend(a.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }));
        }
        Some(dirs::cache_dir()?.join("app.prism.desktop").join(format!("{name}.json")))
    }

    /// The last reading, if it is `provider`'s.
    pub fn load(provider: Provider, account: Option<&str>) -> Option<ClaudeUsage> {
        let saved: Saved = serde_json::from_slice(&std::fs::read(path(provider, account)?).ok()?).ok()?;
        let usage = match saved.provider {
            _ if saved.provider != provider => return None,
            Provider::Claude => super::parse(&saved.body),
            Provider::CommandCode => super::command_code::parse(&saved.body),
            Provider::Codex => super::codex::parse(&saved.body),
            Provider::OpenCode => return None,
        };
        Some(ClaudeUsage { fetched_at: saved.fetched_at, ..usage })
    }

    pub fn save(provider: Provider, account: Option<&str>, body: &serde_json::Value) {
        let Some(path) = path(provider, account) else { return };
        let saved = Saved { provider, fetched_at: Utc::now(), body: body.clone() };
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(path.parent().unwrap_or(&path))?;
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&saved)?)?;
            std::fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            tracing::warn!("usage: cache not saved: {e}");
        }
    }
}

/// `five_hour` and `seven_day` are the two main windows; the others (per model)
/// are kept as is, since the endpoint adds more as plans evolve.
fn parse(body: &serde_json::Value) -> ClaudeUsage {
    let window = |v: &serde_json::Value| -> Option<UsageWindow> {
        Some(UsageWindow {
            utilization: v.get("utilization")?.as_f64()?,
            resets_at: v.get("resets_at").and_then(|r| r.as_str()).and_then(|r| r.parse().ok()),
        })
    };
    let mut others: Vec<(String, UsageWindow)> = body
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| *k != "five_hour" && *k != "seven_day")
        .filter_map(|(k, v)| Some((k.clone(), window(v)?)))
        .collect();
    others.sort_by(|a, b| a.0.cmp(&b.0));
    ClaudeUsage {
        five_hour: body.get("five_hour").and_then(window),
        seven_day: body.get("seven_day").and_then(window),
        others,
        fetched_at: Utc::now(),
    }
}

/// Claude Code's credentials, as JSON. An account's (`home`, its folder): its keychain entry on
/// macOS (`keychain_service`), its `.credentials.json` elsewhere. Without: the keychain's every
/// `Claude Code-credentials*` entry on macOS (a configuration folder of its own has its entry, and
/// an older one may linger), `~/.claude/.credentials.json` (or under `CLAUDE_CONFIG_DIR`) on Linux
/// and Windows.
async fn credentials(home: Option<&Path>) -> Result<Vec<String>> {
    if let Some(home) = home {
        if cfg!(target_os = "macos") {
            let found = crate::platform::secret(&keychain_service(home)).await?;
            return found.map(|c| vec![c]).with_context(|| tr!("no Claude Code credentials for {dir} in the keychain: log in once with that account", dir = home.display()));
        }
        return Ok(vec![tokio::fs::read_to_string(home.join(".credentials.json")).await.with_context(|| tr!("no Claude Code credentials in {dir}: log in once with that account", dir = home.display()))?]);
    }
    if cfg!(target_os = "macos") {
        // The entries' names only: `dump-keychain` without `-d` shows no secret.
        let dump = crate::platform::command(crate::platform::SECURITY).arg("dump-keychain").output().await?;
        let mut services = keychain_services(&String::from_utf8_lossy(&dump.stdout));
        if services.is_empty() {
            services.push(KEYCHAIN_SERVICE.to_string());
        }
        let mut found = Vec::new();
        for s in services {
            found.extend(crate::platform::secret(&s).await?);
        }
        anyhow::ensure!(!found.is_empty(), "{}", tr("no Claude Code credentials in the keychain"));
        return Ok(found);
    }
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"));
    Ok(vec![tokio::fs::read_to_string(dir.join(".credentials.json")).await.context(tr("no Claude Code credentials (.credentials.json)"))?])
}

/// The keychain entry Claude Code keeps its login in when run with `CLAUDE_CONFIG_DIR=home`: its
/// name, then the first 8 hex digits of the SHA-256 of that path as given (seen on a Mac: the
/// entry of `/Users/…/.claude` set as such).
fn keychain_service(home: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(home.to_string_lossy().as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{KEYCHAIN_SERVICE}-{hex}")
}

/// The services of the keychain's `Claude Code-credentials*` entries, in `security dump-keychain`'s
/// listing.
fn keychain_services(dump: &str) -> Vec<String> {
    let mut out: Vec<String> = dump
        .lines()
        .filter_map(|l| l.trim().strip_prefix("\"svce\"<blob>=\"")?.strip_suffix('"').map(String::from))
        .filter(|s| s.starts_with(KEYCHAIN_SERVICE))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The Claude.ai login expiring last, among the credentials found.
fn freshest(creds: Vec<String>) -> Result<OAuth> {
    let logins: Vec<OAuth> = creds.iter().filter_map(|c| serde_json::from_str::<Credentials>(c).ok()?.claude_ai_oauth).collect();
    anyhow::ensure!(!logins.is_empty() || creds.iter().all(|c| serde_json::from_str::<Credentials>(c).is_err()), "{}", tr("no Claude.ai login in Claude Code credentials"));
    logins.into_iter().max_by_key(|o| o.expires_at.unwrap_or(i64::MAX)).context("unreadable Claude Code credentials")
}

async fn access_token(home: Option<&Path>) -> Result<String> {
    let oauth = freshest(credentials(home).await?)?;
    if let Some(expires) = oauth.expires_at.and_then(DateTime::from_timestamp_millis) {
        if expires < Utc::now() {
            return Err(Expired("expired").into());
        }
        if expires < Utc::now() + EXPIRY_MARGIN {
            return Err(Expired("about to expire").into());
        }
    }
    Ok(oauth.access_token)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    claude_ai_oauth: Option<OAuth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuth {
    access_token: String,
    expires_at: Option<i64>,
}

/// Command Code's plan usage, from the API its `/usage` reads (`api.commandcode.ai/alpha/…`,
/// not documented: every field is optional). Its API key, in `~/.commandcode/auth.json`, is read
/// on each request and never leaves this module: neither log nor snapshot nor cache.
///
/// - `whoami`: the organization, if the plan is a team's;
/// - `billing/credits`: the 5 h and week windows (`windowLimits`, used / cap in dollars) and the
///   credit left for the month;
/// - `billing/subscriptions`: the billing period;
/// - `usage/summary`: what the period has consumed.
///
/// The month is a third window: consumed / (consumed + left), as Command Code computes it.
pub(super) mod command_code {
    use prism_base::i18n::tr;
    use std::time::Duration;

    use anyhow::{bail, Context, Result};
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};

    use super::RateLimited;
    use crate::state::{ClaudeUsage, UsageWindow};

    const API: &str = "https://api.commandcode.ai";

    pub async fn fetch(http: &reqwest::Client) -> Result<(ClaudeUsage, Value)> {
        let key = api_key().await?;
        let whoami = get(http, &key, "/alpha/whoami", &[("limits", "1")]).await?;
        let org = whoami.pointer("/org/id").and_then(Value::as_str).map(String::from);
        let org = org.as_deref().map(|o| ("orgId", o));
        let credits = get(http, &key, "/alpha/billing/credits", org.as_slice()).await?;
        let subscription = get(http, &key, "/alpha/billing/subscriptions", org.as_slice()).await?;
        let mut params: Vec<(&str, &str)> = org.into_iter().collect();
        if let Some(since) = subscription.pointer("/data/currentPeriodStart").and_then(Value::as_str) {
            params.push(("since", since));
        }
        let summary = get(http, &key, "/alpha/usage/summary", &params).await?;
        // Only what `parse` reads: amounts and dates, no identity.
        let body = json!({
            "windows": credits.get("windowLimits"),
            "credits": credits.get("credits"),
            "period_end": subscription.pointer("/data/currentPeriodEnd"),
            "consumed": summary.get("totalCost"),
        });
        Ok((parse(&body), body))
    }

    pub fn parse(body: &Value) -> ClaudeUsage {
        let window = |v: Option<&Value>| -> Option<UsageWindow> {
            let v = v?;
            let cap = v.get("cap")?.as_f64().filter(|c| *c > 0.0)?;
            let used = v.get("used").and_then(Value::as_f64).unwrap_or(0.0);
            Some(UsageWindow { utilization: (used / cap * 100.0).clamp(0.0, 100.0), resets_at: v.get("resetAt").and_then(Value::as_i64).and_then(DateTime::from_timestamp_millis) })
        };
        let amount = |k: &str| body.pointer(k).and_then(Value::as_f64).unwrap_or(0.0).max(0.0);
        let left = amount("/credits/monthlyCredits") + amount("/credits/purchasedCredits") + amount("/credits/freeCredits");
        let consumed = amount("/consumed");
        let month = (left + consumed > 0.0).then(|| UsageWindow {
            utilization: consumed / (left + consumed) * 100.0,
            resets_at: body.get("period_end").and_then(Value::as_str).and_then(|d| d.parse().ok()),
        });
        ClaudeUsage {
            five_hour: window(body.pointer("/windows/fiveHour")),
            seven_day: window(body.pointer("/windows/weekly")),
            others: month.map(|m| ("month".to_string(), m)).into_iter().collect(),
            fetched_at: Utc::now(),
        }
    }

    async fn get(http: &reqwest::Client, key: &str, path: &str, params: &[(&str, &str)]) -> Result<Value> {
        let resp = http.get(format!("{API}{path}")).query(params).bearer_auth(key).timeout(Duration::from_secs(15)).send().await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!("{}", tr("Command Code API key rejected: run cmd login"));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let after = resp.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok()).map(Duration::from_secs);
            return Err(RateLimited(after).into());
        }
        if !status.is_success() {
            bail!("Command Code {path}: HTTP {status}");
        }
        resp.json().await.with_context(|| format!("unreadable Command Code {path} response"))
    }

    /// The key `cmd login` saved.
    async fn api_key() -> Result<String> {
        let path = dirs::home_dir().unwrap_or_default().join(".commandcode").join("auth.json");
        let raw = tokio::fs::read(&path).await.context(tr("no Command Code login: run cmd login"))?;
        let auth: Value = serde_json::from_slice(&raw).context("unreadable Command Code credentials")?;
        auth.get("apiKey").and_then(Value::as_str).filter(|k| !k.is_empty()).map(String::from).context(tr("no Command Code login: run cmd login"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_an_accounts_keychain_entry() {
        // The entry a Mac holds for `CLAUDE_CONFIG_DIR=/Users/denis/.claude` (first 8 hex digits of its SHA-256).
        assert_eq!(super::keychain_service(std::path::Path::new("/Users/denis/.claude")), "Claude Code-credentials-71b2374d");
    }

    #[test]
    fn finds_every_claude_code_entry() {
        let dump = "keychain: \"/Users/x/Library/Keychains/login.keychain-db\"\n    \"acct\"<blob>=\"x\"\n    \"svce\"<blob>=\"Claude Code-credentials\"\n    \"svce\"<blob>=\"prism-google\"\n    \"svce\"<blob>=\"Claude Code-credentials-1a2b3c4d\"\n    \"svce\"<blob>=\"Claude Code-credentials\"\n";
        assert_eq!(keychain_services(dump), ["Claude Code-credentials", "Claude Code-credentials-1a2b3c4d"]);
    }

    #[test]
    fn the_login_expiring_last_wins() {
        let login = |token: &str, at: i64| format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}","expiresAt":{at}}}}}"#);
        let oauth = freshest(vec![login("stale", 1_700_000_000_000), login("fresh", 1_800_000_000_000)]).unwrap();
        assert_eq!(oauth.access_token, "fresh");
        assert!(freshest(vec![r#"{"other":1}"#.into()]).err().expect("an error").to_string().contains("no Claude.ai login"));
    }

    /// Readings from Command Code's API (October 2026), reduced to what `fetch` keeps.
    #[test]
    fn parses_command_code_windows_and_month() {
        let body = serde_json::json!({
            "windows": {
                "fiveHour": { "used": 0.850980319, "cap": 14, "exceeded": false, "resetAt": 1791659741817_i64 },
                "weekly": { "used": 10.5196559992, "cap": 35, "exceeded": false, "resetAt": 1792007526155_i64 }
            },
            "credits": { "monthlyCredits": 59.4803440008, "purchasedCredits": 0, "freeCredits": 0 },
            "period_end": "2026-11-07T19:50:30.000Z",
            "consumed": 10.7723058052
        });
        let u = command_code::parse(&body);
        assert_eq!(u.five_hour.as_ref().unwrap().utilization.round(), 6.0);
        assert_eq!(u.seven_day.as_ref().unwrap().utilization.round(), 30.0);
        assert_eq!(u.seven_day.unwrap().resets_at.unwrap().timestamp(), 1792007526);
        let (name, month) = &u.others[0];
        assert_eq!((name.as_str(), month.utilization.round()), ("month", 15.0));
        assert!(month.resets_at.is_some());
        // A plan without windows nor credits: nothing to show, rather than zeros.
        let empty = command_code::parse(&serde_json::json!({ "windows": { "fiveHour": { "used": 1, "cap": 0 } } }));
        assert!(empty.five_hour.is_none() && empty.seven_day.is_none() && empty.others.is_empty());
    }

    #[test]
    fn parses_main_windows_and_keeps_the_others() {
        let body = serde_json::json!({
            "five_hour": { "utilization": 34.0, "resets_at": "2026-10-05T18:00:00Z" },
            "seven_day": { "utilization": 61.5, "resets_at": null },
            "seven_day_opus": { "utilization": 12.0, "resets_at": "2026-10-09T08:00:00Z" },
            "extra": null
        });
        let u = parse(&body);
        assert_eq!(u.five_hour.as_ref().unwrap().utilization, 34.0);
        assert!(u.five_hour.unwrap().resets_at.is_some());
        assert!(u.seven_day.unwrap().resets_at.is_none());
        assert_eq!(u.others.len(), 1);
        assert_eq!(u.others[0].0, "seven_day_opus");
    }
}

/// Codex's plan usage: no endpoint to call, Codex writes it in its transcripts (its rollouts,
/// `~/.codex/sessions/<y>/<m>/<d>/rollout-*.jsonl`) with each request's tokens (`rate_limits`).
/// Read from the last one written, so as of Codex's last request: a window reset since is empty.
pub(super) mod codex {
    use prism_base::i18n::tr;
    use std::{
        io::{Read, Seek, SeekFrom},
        path::{Path, PathBuf},
    };

    use anyhow::{Context, Result};
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};

    use crate::state::{ClaudeUsage, UsageWindow};

    const TAIL_BYTES: u64 = 256 * 1024;
    const FIVE_HOURS: i64 = 300;
    const WEEK: i64 = 7 * 24 * 60;

    /// From its last session's rollout, under `home` (an account's folder; none, its usual one).
    pub async fn fetch(home: Option<PathBuf>) -> Result<(ClaudeUsage, Value)> {
        tokio::task::spawn_blocking(move || {
            let home = home.or_else(|| crate::agents::provider::Provider::Codex.home(None)).context("no home folder")?;
            let rollout = latest_rollout(&home.join("sessions")).context("no Codex session yet")?;
            let limits = last_limits(&rollout).context(tr("no plan usage in Codex's last session yet"))?;
            // Only what `parse` reads: percentages, windows and resets, not the plan nor the credits.
            let body = json!({ "primary": limits.get("primary"), "secondary": limits.get("secondary") });
            Ok((parse(&body), body))
        })
        .await?
    }

    /// Each window by its length: 5 h and the week where expected, any other one by its name.
    pub fn parse(body: &Value) -> ClaudeUsage {
        let mut usage = ClaudeUsage { five_hour: None, seven_day: None, others: Vec::new(), fetched_at: Utc::now() };
        for name in ["primary", "secondary"] {
            let Some(v) = body.get(name).filter(|v| v.is_object()) else { continue };
            let Some(used) = v.get("used_percent").and_then(Value::as_f64) else { continue };
            let resets_at = v.get("resets_at").and_then(Value::as_i64).and_then(|t| DateTime::from_timestamp(t, 0));
            // Reset since Codex's last request: nothing used in the new window yet.
            let window = match resets_at {
                Some(at) if at <= Utc::now() => UsageWindow { utilization: 0.0, resets_at: None },
                _ => UsageWindow { utilization: used.clamp(0.0, 100.0), resets_at },
            };
            match v.get("window_minutes").and_then(Value::as_i64) {
                Some(FIVE_HOURS) => usage.five_hour = Some(window),
                Some(WEEK) => usage.seven_day = Some(window),
                _ => usage.others.push((name.to_string(), window)),
            }
        }
        usage
    }

    /// The rollout written last: in the latest day's folder that has one.
    fn latest_rollout(sessions: &Path) -> Option<PathBuf> {
        let sorted = |dir: &Path| -> Vec<PathBuf> {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
            entries.sort();
            entries.reverse();
            entries
        };
        for year in sorted(sessions).iter().filter(|p| p.is_dir()) {
            for month in sorted(year).iter().filter(|p| p.is_dir()) {
                for day in sorted(month).iter().filter(|p| p.is_dir()) {
                    let latest = sorted(day)
                        .into_iter()
                        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("rollout-")) && p.extension().is_some_and(|e| e == "jsonl"))
                        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
                    if latest.is_some() {
                        return latest;
                    }
                }
            }
        }
        None
    }

    /// The `rate_limits` of the rollout's last `token_count`, read from its end only.
    fn last_limits(rollout: &Path) -> Option<Value> {
        let mut f = std::fs::File::open(rollout).ok()?;
        let len = f.metadata().ok()?.len();
        f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        buf.split(|b| *b == b'\n').rev().find_map(|line| {
            let v: Value = serde_json::from_slice(line).ok()?;
            let payload = v.get("payload").filter(|p| p.get("type").and_then(Value::as_str) == Some("token_count"))?;
            payload.get("rate_limits").filter(|r| r.is_object()).cloned()
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_the_last_rollout_limits() {
            let root = std::env::temp_dir().join(format!("prism-codex-usage-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let (old, new) = (root.join("2026/10/13"), root.join("2026/10/14"));
            std::fs::create_dir_all(&old).unwrap();
            std::fs::create_dir_all(&new).unwrap();
            std::fs::write(old.join("rollout-a.jsonl"), "{}\n").unwrap();
            let soon = Utc::now().timestamp() + 3600;
            let line = format!(r#"{{"type":"event_msg","payload":{{"type":"token_count","rate_limits":{{"primary":{{"used_percent":42.0,"window_minutes":300,"resets_at":{soon}}},"secondary":{{"used_percent":7.5,"window_minutes":10080,"resets_at":1000}},"plan_type":"plus"}}}}}}"#);
            std::fs::write(new.join("rollout-b.jsonl"), format!("{line}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_complete\"}}}}\n")).unwrap();
            let rollout = latest_rollout(&root).unwrap();
            assert!(rollout.ends_with("2026/10/14/rollout-b.jsonl"));
            let limits = last_limits(&rollout).unwrap();
            let usage = parse(&json!({ "primary": limits.get("primary"), "secondary": limits.get("secondary") }));
            assert_eq!(usage.five_hour.map(|w| w.utilization), Some(42.0));
            assert_eq!(usage.seven_day.map(|w| (w.utilization, w.resets_at)), Some((0.0, None)), "reset since: empty");
            assert!(usage.others.is_empty());
            let _ = std::fs::remove_dir_all(&root);
        }
    }
}
