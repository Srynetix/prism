//! The modules' web files for the phone (`/modules/<id>/<path>`), and their list
//! (`/modules/index.json`): a session only, as the page's socket. They are the team's code: never
//! served before the phone is unlocked. Each profile serves its own (relayed under `/p/<id>/`).

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::server::{device_of, Shared};

pub(crate) async fn serve(State(inner): State<Shared>, headers: HeaderMap, Path(rest): Path<String>) -> Response {
    if device_of(&inner, &headers).is_none() {
        return (StatusCode::UNAUTHORIZED, "no session").into_response();
    }
    let found = tokio::task::spawn_blocking(move || {
        if rest == "index.json" {
            return serde_json::to_vec(&prism_core::modules::list()).map(|b| (b, "application/json")).map_err(|e| e.to_string());
        }
        let (id, file) = rest.split_once('/').ok_or("no such file")?;
        prism_core::modules::web_file(id, file)
    })
    .await;
    match found {
        Ok(Ok((body, mime))) => ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-store")], body).into_response(),
        Ok(Err(e)) => (StatusCode::NOT_FOUND, e).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
