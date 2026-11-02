//! GitHub's actions, `gh` answered from memory: what they refuse before calling it, the calls
//! they make, and the snapshot they leave.

use std::sync::Arc;

use prism_core::forge::cli::{fake::Replay, Cli};
use serde_json::{json, Value};

use super::*;
use crate::state::{MergeRequest, MrState, Pipeline, PipelineStatus};

fn actions() -> Actions {
    let actions = Actions::default();
    actions.configure(vec![(None, "acme/shop".into()), (Some("infra".into()), "acme/infra".into())], Vec::new());
    actions
}

fn pr(number: u64) -> MergeRequest {
    MergeRequest { iid: number, state: MrState::Opened, merge_status: "mergeable".into(), ..Default::default() }
}

fn hub_with(prs: Vec<MergeRequest>) -> Hub {
    let hub = Hub::new();
    hub.update(|s| {
        s.merge_requests = prs;
        true
    });
    hub
}

/// The pull request as GitHub has it now (`pull_request_state.graphql`).
fn on_github(state: &str) -> Value {
    json!({ "data": { "repository": { "viewerDefaultMergeMethod": "SQUASH", "pullRequest": {
        "id": "PR_1", "state": state, "isDraft": false, "title": "Fix", "headRefOid": "abc123", "reviewDecision": null,
        "author": { "login": "me" }, "autoMergeRequest": null, "assignees": { "nodes": [] },
        "reviewRequests": { "nodes": [] }, "latestOpinionatedReviews": { "nodes": [] },
    } } } })
}

#[tokio::test]
async fn a_ready_pull_request_is_merged_with_my_method_against_its_head() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![pr(12)]));
    replay.on(Cli::Gh, &["pullRequest(number: $number)", "owner=acme", "name=shop", "number=12"], on_github("OPEN"));
    replay.on(Cli::Gh, &["mergePullRequest"], json!({ "data": { "mergePullRequest": { "pullRequest": { "state": "MERGED" } } } }));
    replay.run(actions().merge_mr(&hub, None, 12)).await.unwrap();
    let merge = replay.calls().into_iter().find(|c| c.line().contains("mergePullRequest")).unwrap();
    for arg in ["id=PR_1", "head=abc123", "method=SQUASH"] {
        assert!(merge.args.iter().any(|a| a == arg), "{arg} in {}", merge.line());
    }
    let m = hub.subscribe().borrow().merge_requests[0].clone();
    assert_eq!(m.state, MrState::Merged);
    assert!(m.merged_at.is_some());
}

#[tokio::test]
async fn a_pull_request_closed_meanwhile_is_not_merged() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![pr(12)]));
    replay.on(Cli::Gh, &["pullRequest(number: $number)"], on_github("CLOSED"));
    let e = replay.run(actions().merge_mr(&hub, None, 12)).await.unwrap_err();
    assert!(e.to_string().contains("no longer open"), "{e}");
    assert!(replay.calls().iter().all(|c| !c.line().contains("mergePullRequest")));
}

#[tokio::test]
async fn a_pull_request_not_ready_is_refused_before_gh() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { draft: true, ..pr(12) }, MergeRequest { merge_status: "checking".into(), ..pr(13) }]);
    for number in [12, 13, 99] {
        assert!(replay.run(actions().merge_mr(&hub, None, number)).await.is_err(), "#{number}");
    }
    assert!(replay.run(actions().merge_mr(&hub, Some("elsewhere"), 12)).await.unwrap_err().to_string().contains("not a repository"));
    assert!(replay.calls().is_empty());
}

#[tokio::test]
async fn another_repository_s_pull_request_is_read_in_its_project() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { repo: Some("infra".into()), ..pr(4) }]);
    replay.on(Cli::Gh, &["pullRequest(number: $number)", "name=infra", "number=4"], on_github("OPEN"));
    replay.on(Cli::Gh, &["mergePullRequest"], json!({ "data": { "mergePullRequest": { "pullRequest": { "state": "MERGED" } } } }));
    replay.run(actions().merge_mr(&hub, Some("infra"), 4)).await.unwrap();
    assert_eq!(hub.subscribe().borrow().merge_requests[0].state, MrState::Merged);
}

#[tokio::test]
async fn a_rebased_pull_request_waits_for_its_checks() {
    let replay = Arc::new(Replay::default());
    let hub = hub_with(vec![MergeRequest { needs_rebase: true, merge_status: "need_rebase".into(), ..pr(12) }]);
    replay.on(Cli::Gh, &["pullRequest(number: $number)"], on_github("OPEN"));
    replay.on(Cli::Gh, &["updatePullRequestBranch"], json!({ "data": { "updatePullRequestBranch": { "clientMutationId": null } } }));
    replay.run(actions().rebase_mr(&hub, None, 12)).await.unwrap();
    let m = hub.subscribe().borrow().merge_requests[0].clone();
    assert_eq!((m.needs_rebase, m.merge_status.as_str()), (false, "checking"));
    // Up to date: nothing to rebase.
    assert!(replay.run(actions().rebase_mr(&hub, None, 12)).await.is_err());
}

#[tokio::test]
async fn graphql_s_errors_reach_the_window() {
    let (replay, hub) = (Arc::new(Replay::default()), hub_with(vec![pr(12)]));
    replay.on(Cli::Gh, &["pullRequest(number: $number)"], on_github("OPEN"));
    replay.on(Cli::Gh, &["mergePullRequest"], json!({ "data": null, "errors": [{ "message": "Base branch was modified" }] }));
    let e = replay.run(actions().merge_mr(&hub, None, 12)).await.unwrap_err();
    assert_eq!(e.to_string(), "GraphQL: Base branch was modified");
    assert_eq!(hub.subscribe().borrow().merge_requests[0].state, MrState::Opened);
}

#[tokio::test]
async fn only_a_pipeline_shown_and_in_the_right_state_is_canceled_or_retried() {
    let replay = Arc::new(Replay::default());
    let pipeline = Pipeline { id: 77, status: PipelineStatus::Success, ..Default::default() };
    let hub = hub_with(vec![MergeRequest { head_pipeline: Some(pipeline), ..pr(12) }]);
    assert!(replay.run(actions().cancel_pipeline(&hub, 77)).await.unwrap_err().to_string().contains("already success"));
    assert!(replay.run(actions().retry_pipeline(&hub, 77)).await.unwrap_err().to_string().contains("nothing to retry"));
    assert!(replay.run(actions().cancel_pipeline(&hub, 5)).await.unwrap_err().to_string().contains("not one Prism displays"));
    assert!(replay.calls().is_empty());
}
