//! Title/keyword search for the magic wand: Semantic Scholar Graph API first,
//! arXiv title search as fallback.
//!
//! Search only maps free text → candidate identifiers. The chosen candidate is
//! fed back into the identifier import pipeline, so Translator stays the single
//! source of truth for metadata.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::core::error::AppError;
use crate::features::refs::latex;

const USER_AGENT: &str =
    "agentero/0.2 (+https://github.com/poco-ai/agentero; mailto:agentero@users.noreply.github.com)";
const SEARCH_CONCURRENCY: usize = 2;

fn search_limiter() -> &'static Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMITER.get_or_init(|| Arc::new(Semaphore::new(SEARCH_CONCURRENCY)))
}

async fn acquire_search_permit() -> OwnedSemaphorePermit {
    search_limiter()
        .clone()
        .acquire_owned()
        .await
        .expect("search limiter should not be closed")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperSearchCandidate {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub citation_count: Option<i64>,
    pub url: Option<String>,
    /// Text handed back to the identifier pipeline (arXiv id preferred over DOI).
    pub identifier: String,
    /// `"s2"` or `"crossref"`.
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperSearchGroup {
    pub query: String,
    pub candidates: Vec<PaperSearchCandidate>,
}

/// Search papers by title/keyword. Returns at most `limit` candidates that carry
/// an arXiv id or DOI (anything else cannot be imported).
///
/// Semantic Scholar first (cross-domain, carries citation counts), arXiv as
/// fallback. S2's key-less search endpoint is aggressively rate limited, so the
/// arXiv path is the common one in practice.
pub async fn search_papers(
    query: &str,
    limit: usize,
) -> Result<Vec<PaperSearchCandidate>, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.max(1);

    match s2_search(query, limit).await {
        Ok(hits) if !hits.is_empty() => return Ok(rank(hits, query, limit)),
        Ok(_) => log::warn!("title search: semantic scholar returned no results for {query}"),
        Err(e) => log::warn!("title search: semantic scholar failed ({e}); falling back to arXiv"),
    }
    match arxiv_search(query, limit).await {
        Ok(hits) => Ok(rank(hits, query, limit)),
        Err(e) => Err(AppError::message(format!("arXiv search failed: {e}"))),
    }
}

/// Keep the provider's relevance order, but float exact title matches to the
/// top — same-named papers otherwise bury the one the user meant.
fn rank(
    mut hits: Vec<PaperSearchCandidate>,
    query: &str,
    limit: usize,
) -> Vec<PaperSearchCandidate> {
    let target = normalize_title(query);
    hits.sort_by_key(|c| normalize_title(&c.title) != target);
    hits.truncate(limit);
    hits
}

fn normalize_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

fn http_client() -> Result<reqwest::Client, String> {
    crate::features::network::client_builder()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("http client: {e}"))
}

async fn get_text(url: &str) -> Result<String, String> {
    let _permit = acquire_search_permit().await;
    let client = http_client()?;
    let res = client
        .get(url)
        .header("Accept", "application/json, application/atom+xml")
        .send()
        .await
        .map_err(|e| format!("request: {e}"))?;
    let status = res.status();
    if !status.is_success() {
        return Err(format!("http {status}"));
    }
    res.text().await.map_err(|e| format!("body: {e}"))
}

async fn get_json(url: &str) -> Result<Value, String> {
    let text = get_text(url).await?;
    serde_json::from_str(&text).map_err(|e| format!("json: {e}"))
}

/// `GET /graph/v1/paper/search?query=…` — relevance-ordered, keeps API order.
async fn s2_search(query: &str, limit: usize) -> Result<Vec<PaperSearchCandidate>, String> {
    let url = format!(
        "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={}&fields=title,authors,year,venue,externalIds,citationCount,url",
        urlencoding::encode(query),
        // Ask for headroom: entries without DOI/arXiv id get dropped below.
        (limit * 4).min(100)
    );
    let value = get_json(&url).await?;
    let Some(items) = value.get("data").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in items {
        let Some(title) = str_field(item, "title") else {
            continue;
        };
        let doi = str_field_at(item, "/externalIds/DOI");
        let arxiv_id = str_field_at(item, "/externalIds/ArXiv")
            .map(|s| latex::strip_arxiv_version(&s).to_string());
        let Some(identifier) = pick_identifier(arxiv_id.as_deref(), doi.as_deref()) else {
            continue;
        };
        out.push(PaperSearchCandidate {
            title,
            authors: item
                .get("authors")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| str_field(a, "name"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            year: item.get("year").and_then(|v| v.as_i64()).map(|y| y as i32),
            venue: str_field(item, "venue"),
            doi,
            arxiv_id,
            citation_count: item.get("citationCount").and_then(|v| v.as_i64()),
            url: str_field(item, "url"),
            identifier,
            source: "s2",
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// `GET https://export.arxiv.org/api/query?search_query=ti:"…"` — Atom feed.
///
/// `map::map_arxiv_atom` parses a single-entry response, so multi-result search
/// splits `<entry>` blocks here.
async fn arxiv_search(query: &str, limit: usize) -> Result<Vec<PaperSearchCandidate>, String> {
    // Quotes would terminate the phrase early and break the query syntax.
    let phrase = query.replace('"', " ");
    let url = format!(
        "https://export.arxiv.org/api/query?search_query={}&start=0&max_results={}&sortBy=relevance",
        urlencoding::encode(&format!("ti:\"{}\"", phrase.trim())),
        (limit * 2).min(50)
    );
    let xml = get_text(&url).await?;

    let mut out = Vec::new();
    for entry in xml.split("<entry>").skip(1) {
        let entry = entry.split("</entry>").next().unwrap_or(entry);
        let Some(title) = tag_text(entry, "title") else {
            continue;
        };
        let Some(arxiv_id) = tag_text(entry, "id")
            .and_then(|id| id.rsplit('/').next().map(str::to_string))
            .map(|id| latex::strip_arxiv_version(&id).to_string())
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let authors = entry
            .split("<author>")
            .skip(1)
            .filter_map(|a| tag_text(a, "name"))
            .collect();
        out.push(PaperSearchCandidate {
            title,
            authors,
            year: tag_text(entry, "published")
                .and_then(|d| d.get(..4).and_then(|y| y.parse::<i32>().ok())),
            venue: tag_text(entry, "arxiv:journal_ref"),
            identifier: arxiv_id.clone(),
            doi: tag_text(entry, "arxiv:doi"),
            arxiv_id: Some(arxiv_id),
            citation_count: None,
            url: None,
            source: "arxiv",
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// First `<tag>…</tag>` in `xml`, whitespace collapsed.
fn tag_text(xml: &str, tag: &str) -> Option<String> {
    let body = xml
        .split(&format!("<{tag}>"))
        .nth(1)?
        .split(&format!("</{tag}>"))
        .next()?;
    let text = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn pick_identifier(arxiv_id: Option<&str>, doi: Option<&str>) -> Option<String> {
    arxiv_id
        .or(doi)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn str_field_at(v: &Value, pointer: &str) -> Option<String> {
    v.pointer(pointer)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(title: &str) -> PaperSearchCandidate {
        PaperSearchCandidate {
            title: title.to_string(),
            authors: Vec::new(),
            year: None,
            venue: None,
            doi: None,
            arxiv_id: Some("0000.00000".to_string()),
            citation_count: None,
            url: None,
            identifier: title.to_string(),
            source: "arxiv",
        }
    }

    #[test]
    fn floats_exact_title_match_to_the_top() {
        let hits = vec![
            candidate("Is Attention All You Need?"),
            candidate("Attention Is All You Need"),
            candidate("Not All Attention Is All You Need"),
        ];
        let ranked = rank(hits, "attention is all you need", 3);
        assert_eq!(ranked[0].title, "Attention Is All You Need");
        // Non-matches keep provider relevance order.
        assert_eq!(ranked[1].title, "Is Attention All You Need?");
    }

    #[test]
    fn normalizes_punctuation_and_case() {
        assert_eq!(
            normalize_title("Attention Is All You Need!"),
            normalize_title("  attention is  all-you-need ")
        );
    }

    #[test]
    fn parses_an_arxiv_atom_entry() {
        let xml = r#"<feed><entry>
          <id>http://arxiv.org/abs/1706.03762v7</id>
          <published>2017-06-12T17:57:34Z</published>
          <title>Attention Is All
  You Need</title>
          <author><name>Ashish Vaswani</name></author>
          <author><name>Noam Shazeer</name></author>
        </entry></feed>"#;
        let entry = xml.split("<entry>").nth(1).unwrap();
        assert_eq!(
            tag_text(entry, "title").as_deref(),
            Some("Attention Is All You Need")
        );
        assert_eq!(
            tag_text(entry, "id").unwrap().rsplit('/').next().unwrap(),
            "1706.03762v7"
        );
    }
}
