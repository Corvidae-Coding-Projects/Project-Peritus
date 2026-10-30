//! Official public Stack Exchange reads; HTML routes remain explicitly selectable.

use crate::{Error, FailureKind, PageFailure, checked_url, extract, http, invalid};
use serde_json::{Value, json};
use std::fmt::Write as _;

pub struct Page {
    pub url: String,
    pub extracted: extract::Extracted,
    pub raw: Vec<u8>,
    pub retrieval: Value,
}

fn target(url: &url::Url) -> Option<(String, bool, u64)> {
    let host = url.host_str()?.trim_start_matches("www.");
    let site = match host {
        "stackoverflow.com" => "stackoverflow".to_owned(),
        "superuser.com" => "superuser".to_owned(),
        "serverfault.com" => "serverfault".to_owned(),
        "askubuntu.com" => "askubuntu".to_owned(),
        "mathoverflow.net" => "mathoverflow".to_owned(),
        h if h.ends_with(".stackoverflow.com") => h.trim_end_matches(".com").to_owned(),
        h if h.ends_with(".stackexchange.com") && h != "api.stackexchange.com" => {
            h.trim_end_matches(".stackexchange.com").to_owned()
        }
        _ => return None,
    };
    let mut parts = url.path_segments()?;
    let kind = parts.next()?;
    if !matches!(kind, "questions" | "q" | "a") {
        return None;
    }
    let id = parts.next()?.parse::<u64>().ok().filter(|id| *id > 0)?;
    Some((site, kind == "a", id))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

struct Reader<'a> {
    site: String,
    cancelled: &'a dyn Fn() -> bool,
    raw: Vec<u8>,
    responses: Vec<Value>,
    continuations: Vec<String>,
    deferred: Vec<Value>,
}

impl Reader<'_> {
    fn remaining(&mut self, path: &str) -> Result<Vec<Value>, Error> {
        match self.get(path, 1) {
            Ok(items) => Ok(items),
            Err(Error::Page(failure)) => {
                let mut url = checked_url(&format!("https://api.stackexchange.com/2.3/{path}"))?;
                url.query_pairs_mut()
                    .append_pair("site", &self.site)
                    .append_pair("filter", "withbody")
                    .append_pair("pagesize", "100")
                    .append_pair("page", "1");
                self.continuations.push(url.to_string());
                self.deferred.push(failure.observation());
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }

    fn get(&mut self, path: &str, page: u64) -> Result<Vec<Value>, Error> {
        let mut url = checked_url(&format!("https://api.stackexchange.com/2.3/{path}"))?;
        url.query_pairs_mut()
            .append_pair("site", &self.site)
            .append_pair("filter", "withbody")
            .append_pair("pagesize", "100")
            .append_pair("page", &page.to_string());
        let download = http::get(url.as_str(), self.cancelled)?;
        if self.raw.len().saturating_add(download.bytes.len()) > http::MAX_BODY {
            return Err(invalid(
                "Stack Exchange bundle exceeds the 64 MiB bound; request a narrower page",
            ));
        }
        let value: Value = serde_json::from_str(&download.text)?;
        if let Some(error) = value["error_message"].as_str() {
            let kind = if value["error_name"].as_str().is_some_and(|s| s.contains("throttle")) {
                FailureKind::RateLimited
            } else {
                FailureKind::Http
            };
            return Err(PageFailure::new(kind, url.as_str(), download.status, error).into());
        }
        let items = value["items"]
            .as_array()
            .ok_or_else(|| invalid("Stack Exchange response has no items array"))?
            .clone();
        self.responses.push(json!({"url":url.as_str(),"raw_sha256":crate::source::digest(&download.bytes),
            "quota_remaining":value["quota_remaining"],"backoff":value["backoff"],"has_more":value["has_more"]}));
        self.raw.extend_from_slice(&(download.bytes.len() as u64).to_le_bytes());
        self.raw.extend_from_slice(&download.bytes);
        if value["has_more"] == true {
            url.query_pairs_mut()
                .clear()
                .append_pair("site", &self.site)
                .append_pair("filter", "withbody")
                .append_pair("pagesize", "100")
                .append_pair("page", &page.saturating_add(1).to_string());
            self.continuations.push(url.to_string());
        }
        // Do not sleep inside a tool or spend another request after an upstream backoff.
        // Subsequent requests fail with the exact cooldown; the successful page remains evidence.
        if let Some(seconds) = value["backoff"].as_u64().filter(|s| *s > 0) {
            let mut failure = PageFailure::new(
                FailureKind::RateLimited,
                url.as_str(),
                429,
                "Stack Exchange API backoff",
            );
            failure.retry_after = Some(seconds.to_string());
            http::defer(url.as_str(), seconds, &failure)?;
        }
        Ok(items)
    }
}

fn post(html: &mut String, value: &Value, label: &str, canonical: &str) -> Result<(), Error> {
    let body = value["body"].as_str().ok_or_else(|| {
        invalid("Stack Exchange post body was absent; no incomplete source retained")
    })?;
    let owner = value["owner"]["display_name"].as_str().unwrap_or("deleted user");
    let license = value["content_license"].as_str().unwrap_or("license not supplied");
    let anchor = value["answer_id"].as_u64().map_or_else(
        || {
            value["comment_id"].as_u64().map(|id| {
                format!("{canonical}#comment{id}_{}", value["post_id"].as_u64().unwrap_or(0))
            })
        },
        |id| Some(format!("{canonical}#{id}")),
    );
    let link = value["link"].as_str().or(anchor.as_deref()).unwrap_or(canonical);
    let _ = write!(
        html,
        "<section><h2>{}</h2><p>Author: {}; License: {}; Score: {}; Accepted: {}</p><a href=\"{}\">Canonical post</a>{body}</section>",
        escape(label),
        escape(owner),
        escape(license),
        value["score"],
        value["is_accepted"],
        escape(link)
    );
    Ok(())
}

pub fn read(url: &str, cancelled: &dyn Fn() -> bool) -> Result<Option<Page>, Error> {
    let parsed = checked_url(url)?;
    let Some((site, answer, id)) = target(&parsed) else {
        return Ok(None);
    };
    let mut reader = Reader {
        site,
        cancelled,
        raw: Vec::new(),
        responses: Vec::new(),
        continuations: Vec::new(),
        deferred: Vec::new(),
    };
    let question_id = if answer {
        reader
            .get(&format!("answers/{id}"), 1)?
            .first()
            .and_then(|item| item["question_id"].as_u64())
            .ok_or_else(|| invalid("Stack Exchange answer is unavailable through the public API"))?
    } else {
        id
    };
    let questions = reader.get(&format!("questions/{question_id}"), 1)?;
    let question = questions
        .first()
        .ok_or_else(|| invalid("Stack Exchange question is unavailable through the public API"))?;
    let canonical = question["link"]
        .as_str()
        .ok_or_else(|| invalid("Stack Exchange question has no canonical link"))?;
    checked_url(canonical)?;
    let title = question["title"].as_str().unwrap_or("Stack Exchange question");
    let mut html = format!(
        "<html><head><title>{}</title></head><body><h1>{}</h1>",
        escape(title),
        escape(title)
    );
    post(&mut html, question, "Question", canonical)?;
    let answers = reader.remaining(&format!("questions/{question_id}/answers"))?;
    let mut ids = vec![question_id];
    for answer in &answers {
        post(&mut html, answer, "Answer", canonical)?;
        if let Some(id) = answer["answer_id"].as_u64() {
            ids.push(id);
        }
    }
    for chunk in ids.chunks(100) {
        let ids = chunk.iter().map(u64::to_string).collect::<Vec<_>>().join(";");
        for comment in reader.remaining(&format!("posts/{ids}/comments"))? {
            post(&mut html, &comment, "Comment", canonical)?;
        }
    }
    if !reader.continuations.is_empty() {
        html.push_str("<p>This snapshot has additional or deferred API pages. Open the official continuations to retrieve them:</p>");
        for url in &reader.continuations {
            let _ = write!(html, "<p><a href=\"{}\">Remaining API page</a></p>", escape(url));
        }
    }
    html.push_str("</body></html>");
    let retrieval = json!({"provider":"stackexchange_api","raw_format":"length_prefixed_api_responses",
        "responses":reader.responses,"continuations":reader.continuations,"deferred":reader.deferred,"answer_count":question["answer_count"],
        "answers_returned":answers.len(),"notice":"Continuations are official API URLs for remaining answers/comments; open them to retrieve every page. This is public API content, not proof that deleted/private content is accessible."});
    Ok(Some(Page {
        url: canonical.into(),
        extracted: extract::html(&html, &checked_url(canonical)?),
        raw: reader.raw,
        retrieval,
    }))
}
