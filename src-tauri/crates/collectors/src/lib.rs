//! Prism's sources: each collector reads one (forge, tracker, CI, clusters, calendar, mail…) into
//! the snapshot, at its pace; the actions the window asks of them go through the same modules.
//! `forge` and `tracker` reach the active profile's provider.

pub mod argocd;
pub mod calendar;
pub mod ci_health;
pub mod clickup;
pub mod mail;
pub mod environments;
pub mod forge;
pub mod github;
pub mod gitlab;
pub mod google;
pub mod jira;
pub mod oauth;
pub mod obsidian;
pub mod passes;
pub mod repos;
pub mod pod_history;
pub mod pods;
pub mod sessions;
pub mod task_view;
pub mod ticket_commits;
pub mod time;
pub mod tracker;
pub mod usage;
pub mod vulnerabilities;
pub mod worktrees;

use prism_base::{git, http, platform};
use prism_core::{agents, blocks, config, conventions, images, profile, stages, state, ticket};
