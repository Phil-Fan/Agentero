//! Title-driven metadata resolver that follows the Bibtex-Verifier pipeline:
//!
//! 1. Semantic Scholar `/paper/search/match`
//! 2. Crossref title search (5 results)
//! 3. OpenAlex title search (5 results)
//! 4. Semantic Scholar ordinary title search (5 results)
//! 5. arXiv title search fallback (5 results)
//!
//! A candidate is accepted when its normalized title is at least 70% similar to
//! the query. When S2 match and a Crossref result describe the same paper
//! (title ≥ 85%, year within 2 years, author surname overlap ≥ 30%), their
//! metadata are merged, with Crossref preferred for volume/issue/pages/publisher.

use std::collections::HashSet;

use serde_json::Value;

use crate::core::error::AppError;
use crate::features::import::map::{self, doi_slug, enrich_remote_urls, local_pdf_meta, PaperMeta};
use crate::features::import::title_search::{
    self, arxiv_search, get_json, normalize_title, s2_search, s2_venue_from_paper, str_field,
    str_field_at,
};
use crate::features::refs::latex;

const MATCH_THRESHOLD: i32 = 70;
const SAME_PAPER_THRESHOLD: i32 = 85;
const AUTHOR_OVERLAP_THRESHOLD: f64 = 0.30;
const YEAR_TOLERANCE: i32 = 2;
const QUERY_LIMIT: usize = 5;
const OPENALEX_MAILTO: &str = "agentero@users.noreply.github.com";

#[derive(Debug, Clone)]
struct ResolvedCandidate {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub publication: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub abstract_text: Option<String>,
    pub score: i32,
}

impl From<title_search::PaperSearchCandidate> for ResolvedCandidate {
    fn from(c: title_search::PaperSearchCandidate) -> Self {
        Self {
            title: c.title,
            authors: c.authors,
            year: c.year,
            publication: c.venue,
            doi: c.doi,
            arxiv_id: c.arxiv_id,
            volume: None,
            issue: None,
            pages: None,
            publisher: None,
            abstract_text: None,
            score: 0,
        }
    }
}

impl From<PaperMeta> for ResolvedCandidate {
    fn from(m: PaperMeta) -> Self {
        Self {
            title: m.title,
            authors: m.authors,
            year: m.year,
            publication: m.publication,
            doi: m.doi,
            arxiv_id: m.arxiv_id,
            volume: m.volume,
            issue: m.issue,
            pages: m.pages,
            publisher: m.publisher,
            abstract_text: m.abstract_text,
            score: 0,
        }
    }
}

/// Resolve metadata for a free-form title query.
pub async fn resolve_metadata_chain(query: &str) -> Result<PaperMeta, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::message("empty title query"));
    }
    let norm_query = normalize_title(query);

    // 1. S2 /paper/search/match
    if let Some(s2) = s2_search_match(query).await? {
        let score = title_similarity(&norm_query, &normalize_title(&s2.title));
        if score >= MATCH_THRESHOLD {
            // 2. Crossref query 5
            let mut crossref = crossref_search_by_title(query, QUERY_LIMIT).await?;
            score_candidates(&mut crossref, &norm_query);
            if let Some(cr) = find_same_paper(&s2, &crossref) {
                let mut merged = merge_candidates(&s2, cr);
                enrich_remote_urls(&mut merged);
                return Ok(merged);
            }
            let mut meta = candidate_to_meta(&s2, "s2-match");
            enrich_remote_urls(&mut meta);
            return Ok(meta);
        }
    }

    // 3. Crossref query 5
    let mut crossref = crossref_search_by_title(query, QUERY_LIMIT).await?;
    score_candidates(&mut crossref, &norm_query);
    if let Some(cr) = best_match(&crossref) {
        if cr.score >= MATCH_THRESHOLD {
            let mut meta = candidate_to_meta(cr, "crossref");
            enrich_remote_urls(&mut meta);
            return Ok(meta);
        }
    }

    // 4. OpenAlex query 5
    let mut openalex = openalex_search_by_title(query, QUERY_LIMIT).await?;
    score_candidates(&mut openalex, &norm_query);
    if let Some(oa) = best_match(&openalex) {
        if oa.score >= MATCH_THRESHOLD {
            let mut meta = candidate_to_meta(oa, "openalex");
            enrich_remote_urls(&mut meta);
            return Ok(meta);
        }
    }

    // 5. S2 ordinary search 5
    let s2_hits = s2_search(query, QUERY_LIMIT)
        .await
        .map_err(|e| AppError::message(format!("s2 search failed: {e}")))?;
    let mut s2_candidates: Vec<ResolvedCandidate> =
        s2_hits.into_iter().map(ResolvedCandidate::from).collect();
    score_candidates(&mut s2_candidates, &norm_query);
    if let Some(s2) = best_match(&s2_candidates) {
        if s2.score >= MATCH_THRESHOLD {
            let mut meta = candidate_to_meta(s2, "s2-search");
            enrich_remote_urls(&mut meta);
            return Ok(meta);
        }
    }

    // 6. arXiv fallback
    let arxiv_hits = arxiv_search(query, QUERY_LIMIT)
        .await
        .map_err(|e| AppError::message(format!("arXiv search failed: {e}")))?;
    let mut arxiv_candidates: Vec<ResolvedCandidate> = arxiv_hits
        .into_iter()
        .map(ResolvedCandidate::from)
        .collect();
    score_candidates(&mut arxiv_candidates, &norm_query);
    if let Some(arx) = best_match(&arxiv_candidates) {
        let mut meta = candidate_to_meta(arx, "arxiv");
        enrich_remote_urls(&mut meta);
        return Ok(meta);
    }

    Err(AppError::message(
        "could not resolve metadata for the given title",
    ))
}

/// S2 `/paper/search/match` returns the single best match for a title.
async fn s2_search_match(query: &str) -> Result<Option<ResolvedCandidate>, AppError> {
    let url = format!(
        "https://api.semanticscholar.org/graph/v1/paper/search/match?query={}&fields=title,authors,year,venue,publicationVenue,journal,externalIds,citationCount,url",
        urlencoding::encode(query)
    );
    let value = match get_json(&url).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("s2 search/match failed: {e}");
            return Ok(None);
        }
    };

    // The endpoint usually returns a single paper object. The `/search` family
    // wraps results in `data`; accept both shapes.
    let item = if let Some(arr) = value.get("data").and_then(|v| v.as_array()) {
        arr.first()
    } else {
        Some(&value)
    };

    let Some(item) = item else {
        return Ok(None);
    };

    // Unlike the regular search endpoint, we keep a match even if it has no
    // DOI/arXiv id, because this path is only used for metadata refresh.
    let title = str_field(item, "title")
        .ok_or_else(|| AppError::message("s2 search/match response missing title"))?;

    let doi = str_field_at(item, "/externalIds/DOI");
    let arxiv_id = str_field_at(item, "/externalIds/ArXiv")
        .map(|s| latex::strip_arxiv_version(&s).to_string());

    Ok(Some(ResolvedCandidate {
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
        publication: s2_venue_from_paper(item),
        doi,
        arxiv_id,
        volume: None,
        issue: None,
        pages: None,
        publisher: None,
        abstract_text: None,
        score: 0,
    }))
}

/// Crossref title search: `query.title` returns relevance-ordered works.
async fn crossref_search_by_title(
    title: &str,
    limit: usize,
) -> Result<Vec<ResolvedCandidate>, AppError> {
    let url = format!(
        "https://api.crossref.org/works?query.title={}&rows={}&select=title,author,published-print,published-online,container-title,volume,issue,page,DOI,publisher,URL,type,abstract",
        urlencoding::encode(title),
        limit
    );
    let value = get_json(&url)
        .await
        .map_err(|e| AppError::message(format!("crossref title search failed: {e}")))?;
    let Some(items) = value.pointer("/message/items").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for item in items {
        let Some(doi) = item.get("DOI").and_then(|v| v.as_str()) else {
            continue;
        };
        // `map_crossref_work` expects the `message` object shape; search items
        // share the same fields.
        match map::map_crossref_work(item, doi) {
            Ok(meta) => out.push(ResolvedCandidate::from(meta)),
            Err(e) => log::debug!("crossref search item skipped: {e}"),
        }
    }
    Ok(out)
}

/// OpenAlex title search.
async fn openalex_search_by_title(
    title: &str,
    limit: usize,
) -> Result<Vec<ResolvedCandidate>, AppError> {
    let url = format!(
        "https://api.openalex.org/works?search={}&per_page={}&select=title,display_name,publication_year,doi,authorships,primary_location,biblio,id&mailto={}",
        urlencoding::encode(title),
        limit,
        OPENALEX_MAILTO
    );
    let value = get_json(&url)
        .await
        .map_err(|e| AppError::message(format!("openalex search failed: {e}")))?;
    let Some(items) = value.get("results").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for item in items {
        if let Some(c) = openalex_item_to_candidate(item) {
            out.push(c);
        }
    }
    Ok(out)
}

fn openalex_item_to_candidate(work: &Value) -> Option<ResolvedCandidate> {
    let title = work
        .get("display_name")
        .or_else(|| work.get("title"))
        .and_then(|v| v.as_str())?
        .to_string();

    let authors: Vec<String> = work
        .get("authorships")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    a.get("author")
                        .and_then(|auth| auth.get("display_name"))
                        .and_then(|v| v.as_str())
                })
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    let year = work
        .get("publication_year")
        .and_then(|v| v.as_i64())
        .map(|y| y as i32);

    let doi = work
        .get("doi")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches("https://doi.org/").to_string());

    let publication = work
        .pointer("/primary_location/source/display_name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            work.pointer("/biblio/venue")
                .and_then(|v| v.as_str())
                .map(String::from)
        });

    let volume = work
        .pointer("/biblio/volume")
        .and_then(|v| v.as_str())
        .map(String::from);
    let issue = work
        .pointer("/biblio/issue")
        .and_then(|v| v.as_str())
        .map(String::from);
    let pages = work.get("biblio").and_then(pages_from_openalex_biblio);

    Some(ResolvedCandidate {
        title,
        authors,
        year,
        publication,
        doi,
        arxiv_id: None,
        volume,
        issue,
        pages,
        publisher: None,
        abstract_text: None,
        score: 0,
    })
}

fn pages_from_openalex_biblio(biblio: &Value) -> Option<String> {
    let first = biblio.get("first_page").and_then(|v| v.as_str())?;
    let last = biblio.get("last_page").and_then(|v| v.as_str());
    match last {
        Some(last) if !last.is_empty() && last != first => Some(format!("{first}--{last}")),
        _ => Some(first.to_string()),
    }
}

fn score_candidates(candidates: &mut [ResolvedCandidate], norm_query: &str) {
    for c in candidates {
        c.score = title_similarity(norm_query, &normalize_title(&c.title));
    }
}

fn best_match(candidates: &[ResolvedCandidate]) -> Option<&ResolvedCandidate> {
    candidates.iter().max_by_key(|c| c.score)
}

fn find_same_paper<'a>(
    target: &ResolvedCandidate,
    candidates: &'a [ResolvedCandidate],
) -> Option<&'a ResolvedCandidate> {
    candidates.iter().find(|c| is_same_paper(target, c))
}

fn is_same_paper(a: &ResolvedCandidate, b: &ResolvedCandidate) -> bool {
    title_similarity(&normalize_title(&a.title), &normalize_title(&b.title)) >= SAME_PAPER_THRESHOLD
        && year_close(a.year, b.year)
        && author_overlap(&a.authors, &b.authors) >= AUTHOR_OVERLAP_THRESHOLD
}

fn year_close(a: Option<i32>, b: Option<i32>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => (x - y).abs() <= YEAR_TOLERANCE,
        _ => true,
    }
}

fn author_surnames(authors: &[String]) -> HashSet<String> {
    authors
        .iter()
        .filter_map(|name| {
            let raw = if name.contains(',') {
                name.split(',').next()?
            } else {
                name.split_whitespace().last()?
            };
            let s = raw
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .collect()
}

fn author_overlap(a: &[String], b: &[String]) -> f64 {
    let sa = author_surnames(a);
    let sb = author_surnames(b);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Token-sort-ratio style similarity (0-100).
fn title_similarity(a: &str, b: &str) -> i32 {
    let sorted = |s: &str| -> String {
        let mut tokens: Vec<&str> = s.split_whitespace().collect();
        tokens.sort_unstable();
        tokens.join(" ")
    };
    let sa = sorted(a);
    let sb = sorted(b);
    let dist = levenshtein_distance(&sa, &sb);
    let max_len = sa.chars().count().max(sb.chars().count());
    (((max_len - dist) * 100).checked_div(max_len).unwrap_or(100)) as i32
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn candidate_to_meta(candidate: &ResolvedCandidate, source: &str) -> PaperMeta {
    let id = candidate
        .arxiv_id
        .clone()
        .or_else(|| candidate.doi.clone().map(|d| doi_slug(&d)))
        .unwrap_or_else(|| crate::features::import::slug_from_stem(&candidate.title));

    let paper_type = if candidate.arxiv_id.is_some() {
        "arxiv"
    } else if candidate.doi.is_some() {
        "article"
    } else {
        "pdf"
    };

    let mut meta = local_pdf_meta(id, candidate.title.clone());
    meta.paper_type = paper_type.into();
    meta.authors = candidate.authors.clone();
    meta.year = candidate.year;
    meta.publication = candidate.publication.clone();
    meta.doi = candidate.doi.clone();
    meta.arxiv_id = candidate.arxiv_id.clone();
    meta.volume = candidate.volume.clone();
    meta.issue = candidate.issue.clone();
    meta.pages = candidate.pages.clone();
    meta.publisher = candidate.publisher.clone();
    meta.abstract_text = candidate.abstract_text.clone();
    meta.meta_source = Some(source.into());
    meta
}

fn merge_candidates(s2: &ResolvedCandidate, crossref: &ResolvedCandidate) -> PaperMeta {
    let base = candidate_to_meta(s2, "s2-match");
    let other = candidate_to_meta(crossref, "crossref");
    merge_metadata(&base, &other)
}

fn merge_metadata(base: &PaperMeta, other: &PaperMeta) -> PaperMeta {
    let mut m = base.clone();
    if other.doi.is_some() {
        m.doi = other.doi.clone();
    }
    if other.year.is_some() {
        m.year = other.year;
        m.date = other.date.clone();
    }
    if other.publication.is_some() {
        m.publication = other.publication.clone();
    }
    if other.volume.is_some() {
        m.volume = other.volume.clone();
    }
    if other.issue.is_some() {
        m.issue = other.issue.clone();
    }
    if other.pages.is_some() {
        m.pages = other.pages.clone();
    }
    if other.publisher.is_some() {
        m.publisher = other.publisher.clone();
    }
    if other.issn.is_some() {
        m.issn = other.issn.clone();
    }
    if other.abstract_text.is_some() {
        m.abstract_text = other.abstract_text.clone();
    }
    if other.html_url.is_some() {
        m.html_url = other.html_url.clone();
        m.source_url = other.source_url.clone();
    }
    if other.meta_source.is_some() {
        m.meta_source = Some("s2+crossref".into());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_similarity_handles_word_order() {
        let a = normalize_title("Attention Is All You Need");
        let b = normalize_title("All You Need Is Attention");
        assert!(title_similarity(&a, &b) >= 90);
    }

    #[test]
    fn author_overlap_computes_jaccard() {
        let a = vec!["Ashish Vaswani".into(), "Noam Shazeer".into()];
        let b = vec!["Vaswani, Ashish".into(), "Niki Parmar".into()];
        let overlap = author_overlap(&a, &b);
        assert!((overlap - 0.33).abs() < 0.01);
    }

    #[tokio::test]
    #[ignore = "live network latency benchmark"]
    async fn live_latency_benchmark() {
        use std::time::Instant;

        let title = "Attention Is All You Need";

        let t0 = Instant::now();
        let s2 = s2_search_match(title).await.unwrap_or(None);
        let s2_elapsed = t0.elapsed();
        println!("S2 /paper/search/match: {s2_elapsed:?}");
        if let Some(c) = &s2 {
            println!(
                "  -> {} (score vs query: {})",
                c.title,
                title_similarity(&normalize_title(title), &normalize_title(&c.title))
            );
        }

        let t0 = Instant::now();
        let cr = crossref_search_by_title(title, QUERY_LIMIT)
            .await
            .unwrap_or_default();
        let cr_elapsed = t0.elapsed();
        println!(
            "Crossref title search: {cr_elapsed:?} ({} results)",
            cr.len()
        );

        let t0 = Instant::now();
        let oa = openalex_search_by_title(title, QUERY_LIMIT)
            .await
            .unwrap_or_default();
        let oa_elapsed = t0.elapsed();
        println!(
            "OpenAlex title search: {oa_elapsed:?} ({} results)",
            oa.len()
        );

        let t0 = Instant::now();
        let s2_hits = s2_search(title, QUERY_LIMIT).await.unwrap_or_default();
        let s2_search_elapsed = t0.elapsed();
        println!(
            "S2 ordinary search: {s2_search_elapsed:?} ({} results)",
            s2_hits.len()
        );

        let t0 = Instant::now();
        let arx = arxiv_search(title, QUERY_LIMIT).await.unwrap_or_default();
        let arx_elapsed = t0.elapsed();
        println!(
            "arXiv title search: {arx_elapsed:?} ({} results)",
            arx.len()
        );

        let t0 = Instant::now();
        match resolve_metadata_chain(title).await {
            Ok(meta) => {
                let full_elapsed = t0.elapsed();
                println!("Full chain (first success): {full_elapsed:?}");
                println!("  -> title: {}", meta.title);
                println!("  -> source: {:?}", meta.meta_source);
            }
            Err(e) => println!("Full chain failed: {e}"),
        }
    }
}
