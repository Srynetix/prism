//! The Status pages block in the demo profile.

use chrono::{DateTime, Duration, Utc};

use super::{Component, Incident, Level, Service, StatusState};

/// GitLab.com with CI/CD pipelines delayed, Claude degraded, GitHub and ClickUp fine.
pub fn state(now: DateTime<Utc>) -> StatusState {
    let ago = |minutes: i64| Some(now - Duration::minutes(minutes));
    let ok = |id: &str, name: &str, url: &str| Service {
        id: id.into(),
        name: name.into(),
        logo: Some(id.into()),
        url: url.into(),
        summary: "All Systems Operational".into(),
        checked_at: ago(1),
        ..Default::default()
    };
    StatusState {
        alerts: true,
        services: vec![
            Service {
                level: Level::Major,
                summary: "Partial Service Disruption".into(),
                components: vec![Component { name: "CI/CD".into(), level: Level::Major }],
                incidents: vec![Incident {
                    id: "demo-ci".into(),
                    name: "CI/CD pipelines delayed or not starting".into(),
                    impact: Level::Major,
                    status: "investigating".into(),
                    url: "https://status.gitlab.com".into(),
                    started_at: ago(48),
                    updated_at: ago(12),
                    update: "Merge train, scheduled and downstream pipelines may be delayed or remain in a created state.".into(),
                    components: vec!["CI/CD".into()],
                    maintenance: false,
                }],
                ..ok("gitlab", "GitLab.com", "https://status.gitlab.com")
            },
            ok("clickup", "ClickUp", "https://status.clickup.com"),
            Service {
                level: Level::Minor,
                summary: "Minor Service Outage".into(),
                components: vec![Component { name: "Claude Code".into(), level: Level::Minor }],
                incidents: vec![Incident {
                    id: "demo-claude".into(),
                    name: "Elevated errors on Claude Code".into(),
                    impact: Level::Minor,
                    status: "monitoring".into(),
                    url: "https://status.claude.com".into(),
                    started_at: ago(95),
                    updated_at: ago(20),
                    update: "A fix has been implemented and we are monitoring the results.".into(),
                    components: vec!["Claude Code".into()],
                    maintenance: false,
                }],
                ..ok("claude", "Claude", "https://status.claude.com")
            },
        ],
    }
}
