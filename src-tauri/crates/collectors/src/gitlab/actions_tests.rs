//! GitLab's actions, `glab` answered from memory: what they refuse before calling it, the call
//! they make, and the snapshot they leave.

use std::sync::Arc;

use prism_core::forge::cli::{fake::Replay, Cli};
use serde_json::json;

use super::*;
use crate::state::{MergeRequest, MrState, Pipeline, PipelineStatus};

const PROJECT: &str = "acme/shop";

fn actions() -> Actions {
    let actions = Actions::default();
    let cfg = GitlabConfig { project: PROJECT.into(), merge_queue_label: "merge-queue".into(), ..Default::default() };
    actions.configure(&cfg, vec![("infra".into(), "acme/infra".into())], Vec::new());
    actions
}

fn mr(iid: u64) -> MergeRequest {
    MergeRequest { iid, state: MrState::Opened, merge_status: "mergeable".into(), ..Default::default() }
}

fn pipeline(id: u64, status: &str) -> Pipeline {
    Pipeline { id, status: PipelineStatus::of(status), failed_jobs: vec!["test".into()], ..Default::default() }
}

fn hub_with(mrs: Vec<MergeRequest>) -> Hub {
    let hub = Hub::new();
    hub.update(|s| {
        s.merge_requests = mrs;
        true
    });
    hub
}

#[tokio::test]
async fn a_ready_mr_is_merged_and_shown_merged() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![mr(12)]));
    replay.on(Cli::Glab, &["--method PUT", "projects/acme%2Fshop/merge_requests/12/merge"], json!({ "state": "merged" }));
    replay.run(actions().merge_mr(&hub, None, 12)).await.unwrap();
    let m = hub.subscribe().borrow().merge_requests[0].clone();
    assert_eq!(m.state, MrState::Merged);
    assert!(m.merged_at.is_some());
}

#[tokio::test]
async fn an_mr_gitlab_does_not_merge_is_an_error() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![mr(12)]));
    replay.on(Cli::Glab, &["merge_requests/12/merge"], json!({ "state": "opened" }));
    let e = replay.run(actions().merge_mr(&hub, None, 12)).await.unwrap_err();
    assert!(e.to_string().contains("did not merge"), "{e}");
    assert_eq!(hub.subscribe().borrow().merge_requests[0].state, MrState::Opened);
}

#[tokio::test]
async fn an_mr_not_ready_is_refused_before_glab() {
    let replay = Arc::new(Replay::default());
    let draft = MergeRequest { draft: true, ..mr(12) };
    let blocked = MergeRequest { merge_status: "ci_must_pass".into(), ..mr(13) };
    let hub = hub_with(vec![draft, blocked]);
    for iid in [12, 13, 99] {
        assert!(replay.run(actions().merge_mr(&hub, None, iid)).await.is_err(), "!{iid}");
    }
    // A repository Prism does not follow.
    assert!(replay.run(actions().merge_mr(&hub, Some("elsewhere"), 12)).await.unwrap_err().to_string().contains("not a repository"));
    assert!(replay.calls().is_empty());
}

#[tokio::test]
async fn another_repository_s_mr_goes_to_its_project() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { repo: Some("infra".into()), ..mr(4) }]);
    replay.on(Cli::Glab, &["projects/acme%2Finfra/merge_requests/4/merge"], json!({ "state": "merged" }));
    replay.run(actions().merge_mr(&hub, Some("infra"), 4)).await.unwrap();
    // The primary repository's !4 is another MR: not known, refused.
    assert!(replay.run(actions().merge_mr(&hub, None, 4)).await.is_err());
    assert_eq!(replay.calls().len(), 1);
}

#[tokio::test]
async fn a_running_pipeline_is_canceled_on_its_mr_too() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { head_pipeline: Some(pipeline(77, "running")), pipeline: Some(PipelineStatus::Running), ..mr(12) }]);
    replay.on(Cli::Glab, &["--method POST", "projects/acme%2Fshop/pipelines/77/cancel"], json!({ "status": "canceled" }));
    replay.run(actions().cancel_pipeline(&hub, 77)).await.unwrap();
    let m = hub.subscribe().borrow().merge_requests[0].clone();
    assert_eq!((m.pipeline, m.head_pipeline.unwrap().status), (Some(PipelineStatus::Canceled), PipelineStatus::Canceled));
}

#[tokio::test]
async fn only_a_pipeline_shown_and_in_the_right_state_is_canceled_or_retried() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { head_pipeline: Some(pipeline(77, "success")), ..mr(12) }]);
    assert!(replay.run(actions().cancel_pipeline(&hub, 77)).await.unwrap_err().to_string().contains("already success"));
    assert!(replay.run(actions().retry_pipeline(&hub, 77)).await.unwrap_err().to_string().contains("nothing to retry"));
    assert!(replay.run(actions().cancel_pipeline(&hub, 5)).await.unwrap_err().to_string().contains("not one Prism displays"));
    assert!(replay.calls().is_empty());
}

#[tokio::test]
async fn a_retried_pipeline_forgets_its_failed_jobs() {
    let replay = Arc::new(Replay::default());
    let hub = Hub::new();
    hub.update(|s| {
        s.pipelines = vec![Pipeline { project: Some("acme/infra".into()), ..pipeline(80, "failed") }];
        true
    });
    replay.on(Cli::Glab, &["projects/acme%2Finfra/pipelines/80/retry"], json!({ "status": "running" }));
    replay.run(actions().retry_pipeline(&hub, 80)).await.unwrap();
    let p = hub.subscribe().borrow().pipelines[0].clone();
    assert_eq!((p.status, p.failed_jobs.len()), (PipelineStatus::Running, 0));
}

#[tokio::test]
async fn glab_s_refusal_reaches_the_window() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![mr(12)]);
    replay.fail(Cli::Glab, &["merge_requests/12/merge"], "405 Method Not Allowed");
    let e = replay.run(actions().merge_mr(&hub, None, 12)).await.unwrap_err();
    assert!(e.to_string().contains("405"), "{e}");
    assert_eq!(hub.subscribe().borrow().merge_requests[0].state, MrState::Opened);
}

#[tokio::test]
async fn a_repository_without_a_merge_queue_gets_no_label() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![mr(12)]);
    assert!(replay.run(actions().set_merge_queue(&hub, None, 12, true)).await.unwrap_err().to_string().contains("no merge queue"));
    assert!(replay.calls().is_empty());
}

#[tokio::test]
async fn auto_merge_is_canceled_whatever_gitlab_answers() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![MergeRequest { auto_merge: true, ..mr(12) }]));
    replay.on(Cli::Glab, &["--method POST", "merge_requests/12/cancel_merge_when_pipeline_succeeds"], json!({ "status": "success" }));
    replay.run(actions().set_auto_merge(&hub, None, 12, false)).await.unwrap();
    assert!(!hub.subscribe().borrow().merge_requests[0].auto_merge);
}

#[tokio::test]
async fn an_auto_merge_gitlab_dropped_with_its_pipeline_is_canceled_all_the_same() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![MergeRequest { auto_merge: true, ..mr(12) }]));
    replay.fail(Cli::Glab, &["cancel_merge_when_pipeline_succeeds"], "HTTP 406");
    replay.on(Cli::Glab, &["--method GET", "projects/acme%2Fshop/merge_requests/12"], json!({ "state": "opened", "merge_when_pipeline_succeeds": false }));
    replay.run(actions().set_auto_merge(&hub, None, 12, false)).await.unwrap();
    assert!(!hub.subscribe().borrow().merge_requests[0].auto_merge);
}

#[tokio::test]
async fn an_auto_merge_still_on_after_a_refused_cancel_is_an_error() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![MergeRequest { auto_merge: true, ..mr(12) }]));
    replay.fail(Cli::Glab, &["cancel_merge_when_pipeline_succeeds"], "HTTP 403");
    replay.on(Cli::Glab, &["--method GET", "merge_requests/12"], json!({ "state": "opened", "merge_when_pipeline_succeeds": true }));
    let e = replay.run(actions().set_auto_merge(&hub, None, 12, false)).await.unwrap_err();
    assert!(e.to_string().contains("403"), "{e}");
    assert!(hub.subscribe().borrow().merge_requests[0].auto_merge);
}

fn queued_actions() -> Actions {
    let actions = Actions::default();
    let cfg = GitlabConfig { project: PROJECT.into(), merge_queue_label: "merge-queue".into(), merge_priority_prefix: "merge-priority::".into(), ..Default::default() };
    let shop = crate::config::RepoInfo {
        name: "shop".into(), project: Some(PROJECT.into()), primary: true, clone: true, clone_missing: false, worktrees: true,
        merge_queue: true, ticket_required: false, pipelines: true, agent: None, forge: None, mr_web: None, workspace: None,
    };
    actions.configure(&cfg, Vec::new(), vec![shop]);
    actions
}

#[tokio::test]
async fn a_merge_priority_replaces_the_mr_s_other_one() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { labels: vec!["merge-queue".into(), "merge-priority::2".into()], ..mr(12) }]);
    replay.on(Cli::Glab, &["--method PUT", "merge_requests/12"], json!({ "labels": ["merge-queue", "merge-priority::0"] }));
    replay.run(queued_actions().set_merge_priority(&hub, None, 12, Some(0))).await.unwrap();
    let line = replay.calls()[0].args.join(" ");
    assert!(line.contains("-f add_labels=merge-priority::0 -f remove_labels=merge-priority::2"), "{line}");
    assert_eq!(hub.subscribe().borrow().merge_requests[0].labels, ["merge-queue", "merge-priority::0"]);
}

#[tokio::test]
async fn no_merge_priority_removes_it_and_the_same_one_calls_nothing() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { labels: vec!["merge-priority::1".into()], ..mr(12) }]);
    replay.run(queued_actions().set_merge_priority(&hub, None, 12, Some(1))).await.unwrap();
    assert!(replay.calls().is_empty());
    replay.on(Cli::Glab, &["merge_requests/12"], json!({ "labels": [] }));
    replay.run(queued_actions().set_merge_priority(&hub, None, 12, None)).await.unwrap();
    assert_eq!(replay.calls()[0].args.join(" ").matches("-f ").count(), 1);
    assert!(replay.calls()[0].args.join(" ").ends_with("-f remove_labels=merge-priority::1"));
}

#[tokio::test]
async fn no_merge_priority_without_its_prefix() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![mr(12)]);
    let e = replay.run(actions().set_merge_priority(&hub, None, 12, Some(0))).await.unwrap_err();
    assert!(e.to_string().contains("merge_priority_prefix"), "{e}");
    assert!(replay.calls().is_empty());
}

#[tokio::test]
async fn a_new_pipeline_is_run_for_an_open_mr_and_shown_at_once() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![MergeRequest { head_pipeline: Some(pipeline(7, "skipped")), ..mr(12) }]));
    replay.on(Cli::Glab, &["--method POST", "projects/acme%2Fshop/merge_requests/12/pipelines"], json!({ "id": 8, "status": "created", "web_url": "https://git/p/8" }));
    replay.run(actions().run_mr_pipeline(&hub, None, 12)).await.unwrap();
    let m = hub.subscribe().borrow().merge_requests[0].clone();
    let p = m.head_pipeline.unwrap();
    assert_eq!((p.id, p.status, p.target.as_str(), p.url.as_str()), (8, PipelineStatus::of("created"), "!12", "https://git/p/8"));
    assert_eq!(m.pipeline, Some(PipelineStatus::of("created")));
}

#[tokio::test]
async fn a_new_pipeline_is_refused_for_an_mr_not_open_before_glab() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { state: MrState::Merged, ..mr(12) }]);
    for iid in [12, 99] {
        assert!(replay.run(actions().run_mr_pipeline(&hub, None, iid)).await.is_err(), "!{iid}");
    }
    assert!(replay.calls().is_empty());
}

#[tokio::test]
async fn custom_emoji_are_read_on_the_project_s_group_page_by_page() {
    let replay = Arc::new(Replay::default());
    let page = |nodes: serde_json::Value, next: Option<&str>| json!({ "data": { "group": { "customEmoji": { "nodes": nodes, "pageInfo": { "hasNextPage": next.is_some(), "endCursor": next } } } } });
    replay.on(Cli::Glab, &["graphql", "path=acme"], page(json!([{ "name": "shipit", "url": "https://gitlab.com/uploads/-/system/custom_emoji/file/1/shipit.png" }]), Some("c1")));
    replay.on(Cli::Glab, &["graphql", "path=acme", "after=c1"], page(json!([{ "name": "lgtm", "url": "https://gitlab.com/uploads/-/system/custom_emoji/file/2/lgtm.gif" }, { "name": "linked", "url": "http://example.org/x.png" }]), None));
    let list = replay.run(actions().custom_emoji(None)).await.unwrap();
    // By name, the one linked without https left out.
    assert_eq!(list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["lgtm", "shipit"]);
    assert_eq!(replay.calls().len(), 2);
    // Kept for the hour: no other call.
    replay.run(actions().custom_emoji(None)).await.unwrap();
    assert_eq!(replay.calls().len(), 2);
}

/// A tag job already run, shown in its branch's pipeline: retried on GitLab, then gone from those
/// already run at once; a job Prism does not show as such is refused before glab.
#[tokio::test]
async fn a_job_already_run_is_run_again() {
    let replay = Arc::new(Replay::default());
    let played = crate::state::PlayedJob { id: 41, name: "tag-release(api:production)".into(), url: String::new(), status: PipelineStatus::Success, finished_at: None };
    let stage = crate::state::Stage { name: "tag".into(), played_jobs: vec![played], ..Default::default() };
    let hub = Hub::new();
    hub.update(|s| {
        s.pipelines = vec![Pipeline { id: 9, target: "main".into(), stages: vec![stage], ..Default::default() }];
        true
    });
    assert!(replay.run(actions().replay_job(&hub, 42)).await.is_err());
    assert!(replay.calls().is_empty());
    replay.on(Cli::Glab, &["projects/acme%2Fshop/jobs/41"], json!({ "id": 41, "name": "tag-release(api:production)", "status": "success", "stage": "tag", "web_url": "https://git/j/41", "pipeline": { "id": 9 } }));
    replay.on(Cli::Glab, &["--method POST", "projects/acme%2Fshop/jobs/41/retry"], json!({ "id": 43, "name": "tag-release(api:production)", "status": "pending", "stage": "tag", "web_url": "https://git/j/43", "pipeline": { "id": 9 } }));
    replay.run(actions().replay_job(&hub, 41)).await.unwrap();
    assert!(hub.subscribe().borrow().pipelines[0].stages[0].played_jobs.is_empty());
}
