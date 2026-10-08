//! A text observation emits physical slices before full-source verification finishes.

use super::QueryArgs;
use crate::{error::{Result, problem},files::stream::TextSource,state::App};
use axum::{body::{Body,Bytes},extract::{Query,State},http::header,response::Response};
use futures_util::stream;
use std::{convert::Infallible,sync::Arc};

pub(super) async fn text(State(app):State<Arc<App>>,Query(args):Query<QueryArgs>) -> Result<Response> {
    if args.workspace!=app.snapshot()?.identity {
        return Err(problem("This file observation belongs to another gateway workspace"));
    }
    let source=TextSource::open(app.project(&args.project)?.root,args.path).await?;
    let slices=stream::unfold(Some(source),|state|async move {
        let mut source=state?;
        let (value,next)=match source.next().await {
            Ok(Some(value)) => (value,Some(source)),
            Ok(None) => return None,
            Err(error) => (serde_json::json!({"error":error.0}),None),
        };
        let mut bytes=serde_json::to_vec(&value).unwrap_or_else(|_|b"{\"error\":\"Could not encode file observation\"}".to_vec());
        bytes.push(b'\n');
        Some((Ok::<_,Infallible>(Bytes::from(bytes)),next))
    });
    Response::builder().header(header::CONTENT_TYPE,"application/x-ndjson; charset=utf-8")
        .body(Body::from_stream(slices)).map_err(problem)
}
