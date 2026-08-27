//! Title/keyword search for the magic wand: Semantic Scholar Graph API first,
//! arXiv title search as fallback.
//!
//! Search maps free text → candidate identifiers. Venue/publication is taken
//! from S2 `publicationVenue.name` (normalized, not truncated) rather than the
//! abbreviated `venue` string or Crossref `container-title` (which clips many
//! conference proceedings). Repository names (`arXiv`, `CoRR`) are discarded.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::core::error::AppError;
use crate::features::refs::latex;

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
    crate::core::http::client(Duration::from_secs(20)).map_err(|e| e.to_string())
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

/// Fetch the published venue for an arXiv paper from Semantic Scholar.
/// Prefers `publicationVenue.name` over the abbreviated `venue` string.
pub async fn fetch_s2_venue_by_arxiv(arxiv_id: &str) -> Option<String> {
    let bare = latex::strip_arxiv_version(arxiv_id);
    fetch_s2_paper_venue(&format!("ARXIV:{bare}")).await
}

/// Fetch venue via `paper/DOI:{doi}`. Skips arXiv-issued DOIs (`10.48550/arXiv.…`)
/// — those should go through `ARXIV:{id}` instead.
pub async fn fetch_s2_venue_by_doi(doi: &str) -> Option<String> {
    let doi = doi.trim();
    if doi.is_empty() || is_arxiv_doi(doi) {
        return None;
    }
    fetch_s2_paper_venue(&format!("DOI:{doi}")).await
}

/// Identifier-first S2 venue lookup: arXiv paper endpoint, then DOI.
pub async fn fetch_s2_venue(arxiv_id: Option<&str>, doi: Option<&str>) -> Option<String> {
    if let Some(id) = arxiv_id.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(venue) = fetch_s2_venue_by_arxiv(id).await {
            return Some(venue);
        }
    }
    if let Some(doi) = doi.map(str::trim).filter(|s| !s.is_empty()) {
        return fetch_s2_venue_by_doi(doi).await;
    }
    None
}

async fn fetch_s2_paper_venue(paper_id: &str) -> Option<String> {
    // Keep the `ARXIV:` / `DOI:` prefix unencoded; encode the rest (slashes in DOIs).
    let (prefix, rest) = paper_id.split_once(':').unwrap_or(("", paper_id));
    let url = if prefix.is_empty() {
        format!(
            "https://api.semanticscholar.org/graph/v1/paper/{}?fields=venue,publicationVenue,journal",
            urlencoding::encode(paper_id)
        )
    } else {
        format!(
            "https://api.semanticscholar.org/graph/v1/paper/{}:{}?fields=venue,publicationVenue,journal",
            prefix,
            urlencoding::encode(rest)
        )
    };
    match get_json(&url).await {
        Ok(value) => s2_venue_from_paper(&value),
        Err(e) => {
            log::debug!(
                target: "agentero::lookup",
                "s2 venue lookup for {paper_id} failed: {e}"
            );
            None
        }
    }
}

/// True for repository / preprint placeholders that should not fill `publication`.
pub fn is_usable_publication(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    let n = t.to_ascii_lowercase();
    !matches!(
        n.as_str(),
        "arxiv" | "arxiv.org" | "corr" | "biorxiv" | "medrxiv" | "preprint" | "preprints"
    ) && !n.starts_with("arxiv:")
}

/// Prefer the more complete of two venue strings. Generic placeholders lose.
/// Equal length keeps `primary` (higher-ranked source).
pub fn better_publication(primary: Option<&str>, other: Option<&str>) -> Option<String> {
    let a = primary.map(str::trim).filter(|s| is_usable_publication(s));
    let b = other.map(str::trim).filter(|s| is_usable_publication(s));
    match (a, b) {
        (Some(x), Some(y)) => {
            // Crossref often clips proceedings titles; keep the longer name.
            if y.len() > x.len() {
                Some(y.to_string())
            } else {
                Some(x.to_string())
            }
        }
        (Some(x), None) => Some(x.to_string()),
        (None, Some(y)) => Some(y.to_string()),
        _ => None,
    }
}

/// Crossref `container-title` for ACL/conference papers is frequently clipped
/// (`Proceedings of the 2019 Conference of the North`). S2 `publicationVenue`
/// usually has the full name, so those strings still need an S2 pass.
pub fn needs_s2_venue_enrichment(publication: Option<&str>) -> bool {
    let p = publication.unwrap_or("").trim();
    !is_usable_publication(p) || p.to_ascii_lowercase().starts_with("proceedings")
}

/// S2 venue in priority order: `publicationVenue.name` (skip repositories),
/// then `journal.name`, then the legacy `venue` string.
pub fn s2_venue_from_paper(v: &Value) -> Option<String> {
    if let Some(pv) = v.get("publicationVenue") {
        let is_repo = pv
            .get("type")
            .and_then(|t| t.as_str())
            .is_some_and(|t| t.eq_ignore_ascii_case("repository"));
        if !is_repo {
            if let Some(name) = str_field(pv, "name").filter(|s| is_usable_publication(s)) {
                return Some(name);
            }
        }
    }
    if let Some(name) = str_field_at(v, "/journal/name").filter(|s| is_usable_publication(s)) {
        return Some(name);
    }
    str_field(v, "venue").filter(|s| is_usable_publication(s))
}

fn is_arxiv_doi(doi: &str) -> bool {
    doi.to_ascii_lowercase().contains("10.48550/arxiv.")
}

/// `GET /graph/v1/paper/search?query=…` — relevance-ordered, keeps API order.
async fn s2_search(query: &str, limit: usize) -> Result<Vec<PaperSearchCandidate>, String> {
    let url = format!(
        "https://api.semanticscholar.org/graph/v1/paper/search?query={}&limit={}&fields=title,authors,year,venue,publicationVenue,journal,externalIds,citationCount,url",
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
        let Some(candidate) = s2_candidate_from_item(item) else {
            continue;
        };
        out.push(candidate);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

fn s2_candidate_from_item(item: &Value) -> Option<PaperSearchCandidate> {
    let title = str_field(item, "title")?;
    let doi = str_field_at(item, "/externalIds/DOI");
    let arxiv_id = str_field_at(item, "/externalIds/ArXiv")
        .map(|s| latex::strip_arxiv_version(&s).to_string());
    let identifier = pick_identifier(arxiv_id.as_deref(), doi.as_deref())?;
    Some(PaperSearchCandidate {
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
        venue: s2_venue_from_paper(item),
        doi,
        arxiv_id,
        citation_count: item.get("citationCount").and_then(|v| v.as_i64()),
        url: str_field(item, "url"),
        identifier,
        source: "s2",
    })
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

    #[test]
    fn s2_venue_prefers_publication_venue_name() {
        let paper = serde_json::json!({
            "venue": "NeurIPS",
            "publicationVenue": {
                "id": "d9720b90-d60b-48bc-9df8-87a30b9a60dd",
                "name": "Neural Information Processing Systems",
                "type": "conference",
                "alternate_names": ["NeurIPS", "NIPS"]
            },
            "journal": { "pages": "5998-6008" }
        });
        assert_eq!(
            s2_venue_from_paper(&paper).as_deref(),
            Some("Neural Information Processing Systems")
        );
    }

    #[test]
    fn s2_venue_skips_repository_and_uses_journal_name() {
        let paper = serde_json::json!({
            "venue": "arXiv.org",
            "publicationVenue": {
                "name": "arXiv.org",
                "type": "repository"
            },
            "journal": { "name": "Nature", "volume": "596" }
        });
        assert_eq!(s2_venue_from_paper(&paper).as_deref(), Some("Nature"));
    }

    #[test]
    fn s2_venue_falls_back_to_legacy_venue_string() {
        let paper = serde_json::json!({ "venue": "ICML" });
        assert_eq!(s2_venue_from_paper(&paper).as_deref(), Some("ICML"));
    }

    #[test]
    fn s2_venue_rejects_generic_placeholders() {
        let paper = serde_json::json!({
            "venue": "arXiv",
            "publicationVenue": { "name": "CoRR", "type": "journal" },
            "journal": { "name": "bioRxiv" }
        });
        assert_eq!(s2_venue_from_paper(&paper), None);
    }

    #[test]
    fn usable_publication_rejects_preprint_labels() {
        assert!(is_usable_publication("Nature"));
        assert!(is_usable_publication(
            "Advances in Neural Information Processing Systems 30 (NeurIPS 2017)"
        ));
        assert!(!is_usable_publication("arXiv"));
        assert!(!is_usable_publication("  ArXiv.org  "));
        assert!(!is_usable_publication("CoRR"));
        assert!(!is_usable_publication(""));
    }

    #[test]
    fn better_publication_keeps_the_longer_complete_name() {
        assert_eq!(
            better_publication(
                Some("Proceedings of the 2019 Conference of the North"),
                Some("North American Chapter of the Association for Computational Linguistics"),
            )
            .as_deref(),
            Some("North American Chapter of the Association for Computational Linguistics")
        );
        assert_eq!(
            better_publication(
                Some("Advances in Neural Information Processing Systems 30 (NeurIPS 2017)"),
                Some("Neural Information Processing Systems"),
            )
            .as_deref(),
            Some("Advances in Neural Information Processing Systems 30 (NeurIPS 2017)")
        );
        assert_eq!(
            better_publication(Some("arXiv"), Some("Nature")).as_deref(),
            Some("Nature")
        );
        assert_eq!(better_publication(Some("arXiv"), Some("CoRR")), None);
    }

    #[test]
    fn proceedings_titles_still_need_s2_enrichment() {
        assert!(needs_s2_venue_enrichment(None));
        assert!(needs_s2_venue_enrichment(Some("arXiv")));
        assert!(needs_s2_venue_enrichment(Some(
            "Proceedings of the 2019 Conference of the North"
        )));
        assert!(!needs_s2_venue_enrichment(Some("Nature")));
        assert!(!needs_s2_venue_enrichment(Some(
            "Neural Information Processing Systems"
        )));
    }

    #[test]
    fn s2_search_item_uses_publication_venue_when_venue_is_empty() {
        let item = serde_json::json!({
            "title": "BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding",
            "authors": [{ "name": "Jacob Devlin" }],
            "year": 2019,
            "venue": "",
            "publicationVenue": {
                "name": "North American Chapter of the Association for Computational Linguistics",
                "type": "conference"
            },
            "externalIds": {
                "DOI": "10.18653/v1/N19-1423",
                "ArXiv": "1810.04805"
            },
            "citationCount": 90000,
            "url": "https://www.semanticscholar.org/paper/bert"
        });
        let candidate = s2_candidate_from_item(&item).expect("candidate");
        assert_eq!(
            candidate.venue.as_deref(),
            Some("North American Chapter of the Association for Computational Linguistics")
        );
        assert_eq!(candidate.identifier, "1810.04805");
        assert_eq!(candidate.doi.as_deref(), Some("10.18653/v1/N19-1423"));
        assert_eq!(candidate.source, "s2");
    }

    #[test]
    fn arxiv_doi_is_detected() {
        assert!(is_arxiv_doi("10.48550/arXiv.1706.03762"));
        assert!(is_arxiv_doi("10.48550/ARXIV.1810.04805"));
        assert!(!is_arxiv_doi("10.1038/s41586-021-03819-2"));
        assert!(!is_arxiv_doi("10.18653/v1/N19-1423"));
    }
}
