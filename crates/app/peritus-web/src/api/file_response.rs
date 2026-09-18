//! Project-confined streaming responses and media-specific browser policy.

use super::QueryArgs;
use crate::{
    error::{Result, problem},
    files,
    state::App,
};
use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::{HeaderValue, header},
    response::Response,
};
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub(super) async fn raw(
    State(app): State<Arc<App>>,
    Query(args): Query<QueryArgs>,
    request: Request,
) -> Result<Response> {
    let path = files::resolve(&app.project(&args.project)?.root, &args.path)?;
    if !path.is_file() {
        return Err(problem("Choose a regular file"));
    }
    if args.kind == "pdf" {
        let candidate = path.clone();
        tokio::task::spawn_blocking(move || files::pdf::inspect(&candidate))
            .await
            .map_err(problem)??;
    }
    let mut response =
        ServeFile::new(&path).oneshot(request).await.map_err(problem)?.map(Body::new);
    response
        .headers_mut()
        .insert("content-security-policy", HeaderValue::from_static("sandbox; default-src 'none'"));
    if args.kind == "pdf" {
        // A native PDF viewer cannot portably load under HTML sandbox restrictions.
        // Only verified PDF input gets this policy, forced MIME and no sniffing.
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
        response.headers_mut().insert(
            "content-security-policy",
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'self'; base-uri 'none'"),
        );
    }
    if args.kind == "download" {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
    }
    Ok(response)
}
