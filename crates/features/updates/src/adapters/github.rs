//! GitHub Releases over the hyper + rustls stack kube already pulls in.
//!
//! One read-only `GET` to `api.github.com`, a short timeout, no redirects, a
//! bounded body. Nothing is sent besides the `User-Agent` GitHub requires.

use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Empty, Limited};
use hyper::Request;
use hyper::header::{ACCEPT, USER_AGENT};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;
use thiserror::Error;

use crate::domain::{Release, parse_tag};

/// The only endpoint contacted. `/releases/latest` skips pre-releases, and
/// every NatiKube release is one during the alpha.
pub const RELEASES_URL: &str = "https://api.github.com/repos/paulpwo/NatiKube/releases?per_page=10";

const RELEASE_PAGE_PREFIX: &str = "https://github.com/paulpwo/NatiKube/releases/";
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BODY: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("tls setup: {0}")]
    Tls(String),
    #[error("request: {0}")]
    Request(String),
    #[error("GitHub answered HTTP {0}")]
    Status(u16),
    #[error("response body: {0}")]
    Body(String),
    #[error("response is not valid release JSON: {0}")]
    Json(String),
}

/// `NatiKube/<version>`: the header GitHub requires, with no other identifier.
pub fn user_agent(version: &str) -> String {
    format!("NatiKube/{version}")
}

/// Fetches and parses the latest releases. Callers treat every error as
/// "no update"; the error only feeds a debug log line.
pub async fn fetch_releases(version: &str) -> Result<Vec<Release>, FetchError> {
    tokio::time::timeout(TIMEOUT, fetch(version))
        .await
        .map_err(|_| FetchError::Timeout(TIMEOUT))?
}

async fn fetch(version: &str) -> Result<Vec<Release>, FetchError> {
    let https = HttpsConnectorBuilder::new()
        .with_provider_and_native_roots(rustls::crypto::ring::default_provider())
        .map_err(|e| FetchError::Tls(e.to_string()))?
        .https_only()
        .enable_http1()
        .build();
    let client: Client<_, Empty<Bytes>> = Client::builder(TokioExecutor::new()).build(https);
    let request = Request::get(RELEASES_URL)
        .header(USER_AGENT, user_agent(version))
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .body(Empty::new())
        .map_err(|e| FetchError::Request(e.to_string()))?;
    let response = client
        .request(request)
        .await
        .map_err(|e| FetchError::Request(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(FetchError::Status(status.as_u16()));
    }
    let body = Limited::new(response.into_body(), MAX_BODY)
        .collect()
        .await
        .map_err(|e| FetchError::Body(e.to_string()))?
        .to_bytes();
    let text = std::str::from_utf8(&body).map_err(|e| FetchError::Body(e.to_string()))?;
    parse_releases(text)
}

/// The fields of a GitHub release this app reads; everything else is ignored.
#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
}

/// Parses the `/releases` array. Drafts, tags that are not versions and
/// entries of the wrong shape are skipped one by one, so a single odd release
/// never hides the rest.
pub fn parse_releases(json: &str) -> Result<Vec<Release>, FetchError> {
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| FetchError::Json(e.to_string()))?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<RawRelease>(entry).ok())
        .filter(|raw| !raw.draft)
        .filter_map(|raw| {
            let version = parse_tag(&raw.tag_name)?;
            // Only ever open a page of this repository's releases; anything
            // else in the payload falls back to the tag's own page.
            let url = raw
                .html_url
                .filter(|url| url.starts_with(RELEASE_PAGE_PREFIX))
                .unwrap_or_else(|| format!("{RELEASE_PAGE_PREFIX}tag/{}", raw.tag_name));
            Some(Release {
                tag: raw.tag_name,
                version,
                name: raw
                    .name
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty()),
                notes: raw.body.unwrap_or_default(),
                url,
            })
        })
        .collect())
}
