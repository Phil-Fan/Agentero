//! Turn RSS excerpts / article HTML into Markdown for the plaza detail page.

use super::parse::strip_html;
use scraper::{Html, Selector};
use url::Url;

const ARTICLE_CLASS_HINTS: &[&str] = &[
    "post-content",
    "entry-content",
    "article-content",
    "article-body",
    "post-body",
    "post-inner",
    "content-body",
    "prose",
];

const POSITIVE_HINTS: &[&str] = &[
    "article", "content", "post", "entry", "body", "text", "prose", "story", "blog", "main",
];

const NEGATIVE_HINTS: &[&str] = &[
    "sidebar", "nav", "footer", "header", "aside", "comment", "share", "related", "advert",
    "social", "menu", "widget", "promo", "banner",
];

const CANDIDATE_SELECTOR: &str = "article, main, div, section";
const MIN_CONTENT_CHARS: usize = 40;

pub fn html_to_markdown(html: &str) -> String {
    let (processed, math_blocks) = extract_latex_math(html);
    let md = htmd::convert(&processed).unwrap_or_else(|_| strip_html(&processed));
    let restored = restore_latex_math(&md, &math_blocks);
    restored.replace('\u{200b}', "").trim().to_string()
}

/// LaTeX block environments to preserve (equation, align, aligned, etc.).
const LATEX_BLOCK_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "aligned",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "alignat",
    "alignat*",
    "flalign",
    "flalign*",
];

/// Environments KaTeX cannot render (MathJax-only). Their wrappers are
/// stripped so the inner math still displays; supported envs (align, gather,
/// aligned, …) keep their wrappers inside `$$…$$`.
const KATEX_UNSUPPORTED_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "multline",
    "multline*",
    "flalign",
    "flalign*",
];

/// Extract LaTeX math from HTML before htmd conversion.
///
/// htmd strips backslashes, turning `\boldsymbol` into `boldsymbol`.
/// We pull out `\begin{env}...\end{env}` blocks and `$...$` inline math,
/// replace them with text placeholders, then restore them as
/// `$$...$$` / `$...$` after conversion.
fn extract_latex_math(html: &str) -> (String, Vec<(String, String)>) {
    let mut out = String::with_capacity(html.len());
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut rest = html;

    while !rest.is_empty() {
        // Try to find the earliest LaTeX construct
        let mut earliest: Option<(usize, usize, String)> = None;

        // Find \begin{env}
        if let Some(idx) = rest.find("\\begin{") {
            let brace = idx + "\\begin{".len();
            if let Some(end_brace) = rest[brace..].find('}') {
                let env_name = &rest[brace..brace + end_brace];
                if LATEX_BLOCK_ENVS.contains(&env_name) {
                    let end_tag = format!("\\end{{{}}}", env_name);
                    if let Some(end_idx) = rest[idx..].find(&end_tag) {
                        let abs_end = idx + end_idx + end_tag.len();
                        let content = rest[idx..abs_end].to_string();
                        let pos = earliest.as_ref().is_none_or(|(p, _, _)| idx < *p);
                        if pos {
                            earliest = Some((idx, abs_end, content));
                        }
                    }
                }
            }
        }

        // Find $...$ inline math (not $$...$$)
        let mut search_from = 0;
        while let Some(dollar) = rest[search_from..].find('$') {
            let abs_dollar = search_from + dollar;
            // Skip $$ (display math delimiter)
            if abs_dollar + 1 < rest.len() && rest.as_bytes()[abs_dollar + 1] == b'$' {
                search_from = abs_dollar + 2;
                continue;
            }
            // Find closing $
            let after = abs_dollar + 1;
            if let Some(end_dollar) = rest[after..].find('$') {
                let abs_end = after + end_dollar + 1;
                // Make sure it's not $$ on the closing side either
                if abs_end < rest.len() && rest.as_bytes()[abs_end] == b'$' {
                    search_from = abs_end + 1;
                    continue;
                }
                let content = rest[abs_dollar..abs_end].to_string();
                let pos = earliest.as_ref().is_none_or(|(p, _, _)| abs_dollar < *p);
                if pos {
                    earliest = Some((abs_dollar, abs_end, content));
                }
                break;
            }
            break;
        }

        match earliest {
            Some((start, end, content)) => {
                out.push_str(&rest[..start]);
                // Fixed-width index: a bare `LATEXBLOCK1` placeholder would
                // prefix-match inside `LATEXBLOCK10` during restore.
                let placeholder = format!("LATEXBLOCK{:04}", blocks.len());
                let marker = block_math_marker(&content);
                blocks.push((placeholder.clone(), marker));
                out.push_str(&placeholder);
                rest = &rest[end..];
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }

    (out, blocks)
}

/// Display-math marker for an extracted LaTeX construct. Inline `$…$` passes
/// through; `\begin{env}…\end{env}` becomes `$$…$$`, with the wrapper dropped
/// for environments KaTeX cannot render.
fn block_math_marker(content: &str) -> String {
    let Some(open) = content.strip_prefix("\\begin{") else {
        return sanitize_math_html(content);
    };
    let Some(env) = open.split('}').next() else {
        return format!("$$ {} $$", sanitize_math_html(content));
    };
    if KATEX_UNSUPPORTED_ENVS.contains(&env) {
        let open_len = "\\begin{".len() + env.len() + 1;
        let close_len = "\\end{".len() + env.len() + 1;
        let inner = &content[open_len..content.len().saturating_sub(close_len)];
        format!("$$ {} $$", sanitize_math_html(inner.trim()))
    } else {
        format!("$$ {} $$", sanitize_math_html(content))
    }
}

/// Blogs embed HTML artifacts inside raw LaTeX (`<br />` line breaks,
/// `&lt;` entities, stray tags). Extraction runs before htmd, so clean the
/// math content itself: br → space, drop residual tags, decode entities.
fn sanitize_math_html(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(lt) = rest.find('<') {
        let Some(gt) = rest[lt..].find('>') else {
            break;
        };
        let tag = &rest[lt..lt + gt + 1];
        out.push_str(&rest[..lt]);
        if tag.starts_with("<br") {
            out.push(' ');
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Restore LaTeX math placeholders after htmd conversion.
fn restore_latex_math(md: &str, blocks: &[(String, String)]) -> String {
    let mut result = md.to_string();
    for (placeholder, marker) in blocks {
        result = result.replace(placeholder, marker);
    }
    result
}

/// Paper landing pages (arXiv abs / DOI) are not useful article HTML.
#[allow(dead_code)]
pub fn is_paper_landing_url(url: Option<&str>) -> bool {
    let Some(raw) = url.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(parsed) = Url::parse(raw) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    matches!(
        host.as_str(),
        "arxiv.org"
            | "www.arxiv.org"
            | "rss.arxiv.org"
            | "export.arxiv.org"
            | "doi.org"
            | "www.doi.org"
            | "dx.doi.org"
    ) || host.ends_with(".arxiv.org")
}

/// DOI scraped from an article page's `<meta>` tags: Highwire `citation_doi`
/// first, then `prism.doi` / `dc.identifier` (both tolerate a `doi:` prefix).
/// Used to backfill `items.paper_url` for feeds that never expose a DOI.
pub fn extract_paper_doi(html: &str) -> Option<String> {
    for name in ["citation_doi", "prism.doi", "dc.identifier"] {
        let Some(content) = meta_named(html, name) else {
            continue;
        };
        let doi = content
            .trim()
            .strip_prefix("doi:")
            .map(str::trim)
            .unwrap_or(content.trim());
        if looks_like_doi(doi) {
            return Some(doi.to_string());
        }
    }
    None
}

fn looks_like_doi(s: &str) -> bool {
    !s.is_empty() && s.starts_with("10.") && s.contains('/') && !s.contains(char::is_whitespace)
}

fn meta_named(html: &str, name: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<meta") {
        let start = from + rel;
        let Some(gt) = lower[start..].find('>') else {
            break;
        };
        let end = start + gt + 1;
        let tag_l = &lower[start..end];
        if attr_value(tag_l, "name").as_deref() == Some(name) {
            // Read the content from the original slice: DOIs are
            // case-insensitive but some contain uppercase characters.
            if let Some(content) = attr_value(&html[start..end], "content") {
                if !content.trim().is_empty() {
                    return Some(content);
                }
            }
        }
        from = end;
    }
    None
}

pub fn is_fetchable_http_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url.trim()) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
}

/// Find the main article content using DOM-based Readability scoring.
///
/// Parses HTML into a proper DOM tree, scores candidate containers by
/// class/id hints, paragraph count, text density, and link density, then
/// returns the inner HTML of the best match.  Falls back to string-based
/// tag matching when the DOM approach yields nothing useful.
pub fn extract_article_html(html: &str) -> String {
    let cleaned = strip_noise_tags(html);
    if let Some(dom_result) = dom_extract_article(&cleaned) {
        if strip_html(&dom_result).len() > MIN_CONTENT_CHARS {
            return dom_result;
        }
    }
    fallback_extract_article(&cleaned)
}

fn dom_extract_article(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let candidate_sel = Selector::parse(CANDIDATE_SELECTOR).ok()?;

    let mut best: Option<scraper::ElementRef<'_>> = None;
    let mut best_score = 0.0f64;
    let mut best_depth = 0usize;

    for element in doc.select(&candidate_sel) {
        if is_noise_element(&element) {
            continue;
        }
        let text: String = element.text().collect();
        let text_len = text.trim().chars().count();
        if text_len < MIN_CONTENT_CHARS {
            continue;
        }
        let score = score_candidate(&element, text_len);
        let depth = element.ancestors().count();
        if score > best_score || (score == best_score && depth > best_depth) {
            best = Some(element);
            best_score = score;
            best_depth = depth;
        }
    }

    best.map(|e| e.inner_html())
}

fn is_noise_element(element: &scraper::ElementRef) -> bool {
    let tag = element.value().name.local.as_ref();
    if matches!(
        tag,
        "nav" | "footer" | "aside" | "header" | "script" | "style" | "noscript" | "svg" | "iframe"
    ) {
        return true;
    }
    for attr in element.value().attrs() {
        let name: &str = attr.0;
        if name == "class" || name == "id" {
            let val: &str = attr.1;
            let lower = val.to_ascii_lowercase();
            if NEGATIVE_HINTS.iter().any(|h| lower.contains(h)) {
                return true;
            }
        }
    }
    false
}

fn score_candidate(element: &scraper::ElementRef, text_len: usize) -> f64 {
    let mut score = 0.0f64;

    for attr in element.value().attrs() {
        let name: &str = attr.0;
        if name == "class" || name == "id" {
            let val: &str = attr.1;
            let lower = val.to_ascii_lowercase();
            for hint in POSITIVE_HINTS {
                if lower.contains(hint) {
                    score += 25.0;
                }
            }
            for hint in NEGATIVE_HINTS {
                if lower.contains(hint) {
                    score -= 25.0;
                }
            }
        }
    }

    let p_sel = Selector::parse("p").unwrap();
    let p_count = element.select(&p_sel).count();
    score += (p_count as f64) * 3.0;

    let h_sel = Selector::parse("h1, h2, h3, h4, h5, h6").unwrap();
    let h_count = element.select(&h_sel).count();
    score += (h_count as f64) * 1.0;

    score += (text_len.min(2000) as f64) / 100.0;

    let a_sel = Selector::parse("a").unwrap();
    let link_text: usize = element
        .select(&a_sel)
        .map(|a| a.text().collect::<String>().len())
        .sum();
    if text_len > 0 {
        let density = link_text as f64 / text_len as f64;
        score -= density * 30.0;
    }

    score
}

fn fallback_extract_article(html: &str) -> String {
    if let Some(inner) = extract_element(html, "article") {
        if strip_html(&inner).len() > 20 {
            return inner;
        }
    }
    if let Some(inner) = extract_by_class(html, ARTICLE_CLASS_HINTS) {
        if strip_html(&inner).len() > 40 {
            return inner;
        }
    }
    if let Some(inner) = extract_element(html, "main") {
        if strip_html(&inner).len() > 40 {
            return inner;
        }
    }
    extract_element(html, "body").unwrap_or_else(|| html.to_string())
}

/// Drop RSS “read more” tails (`[...]`, `…`) so the detail page is not a teaser.
pub fn strip_trailing_ellipsis(text: &str) -> String {
    let mut t = text.trim().to_string();
    loop {
        let lower = t.to_ascii_lowercase();
        let cut = if lower.ends_with("[...]") {
            t.len().checked_sub(5)
        } else if t.ends_with("[…]") {
            t.len().checked_sub("[…]".len())
        } else if lower.ends_with("[..]") {
            t.len().checked_sub(4)
        } else if t.ends_with("...") {
            t.len().checked_sub(3)
        } else if t.ends_with('…') {
            t.len().checked_sub('…'.len_utf8())
        } else if lower.ends_with("read more") {
            t.len().checked_sub("read more".len())
        } else if lower.ends_with("continue reading") {
            t.len().checked_sub("continue reading".len())
        } else {
            None
        };
        let Some(end) = cut.filter(|&n| t.is_char_boundary(n)) else {
            break;
        };
        t = t[..end].trim_end().to_string();
    }
    t
}

/// Make the article start with `# Title` so the detail page can render as one
/// Markdown document (no separate HTML heading).
pub fn ensure_heading(md: &str, title: &str) -> String {
    let title = title.trim();
    let md = strip_trailing_ellipsis(md);
    if title.is_empty() {
        return md;
    }
    let heading = format!("# {title}");
    if md.is_empty() {
        return heading;
    }
    let mut lines = md.lines();
    let first = lines.next().unwrap_or("").trim();
    let first_text = first.trim_start_matches('#').trim();
    if first_text.eq_ignore_ascii_case(title) {
        let rest = lines.collect::<Vec<_>>().join("\n").trim().to_string();
        if rest.is_empty() {
            return heading;
        }
        return format!("{heading}\n\n{rest}");
    }
    format!("{heading}\n\n{md}")
}

fn strip_noise_tags(html: &str) -> String {
    let mut rest = html;
    let mut out = String::with_capacity(html.len());
    let pair_tags = ["script", "style", "noscript", "svg", "iframe"];
    while !rest.is_empty() {
        let lower = rest.to_ascii_lowercase();
        if let Some(comment) = lower.find("<!--") {
            out.push_str(&rest[..comment]);
            if let Some(end) = lower[comment + 4..].find("-->") {
                rest = &rest[comment + 4 + end + 3..];
                continue;
            }
            break;
        }
        let mut next: Option<(usize, &str)> = None;
        for tag in pair_tags {
            let open = format!("<{tag}");
            if let Some(idx) = lower.find(&open) {
                if next.is_none_or(|(i, _)| idx < i) {
                    next = Some((idx, tag));
                }
            }
        }
        let Some((idx, tag)) = next else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..idx]);
        let close = format!("</{tag}>");
        if let Some(end) = lower[idx..].find(&close) {
            rest = &rest[idx + end + close.len()..];
        } else if let Some(gt) = rest[idx..].find('>') {
            rest = &rest[idx + gt + 1..];
        } else {
            break;
        }
    }
    out
}

fn extract_element(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let start = lower.find(&open)?;
    let gt = lower[start..].find('>')?;
    let inner_start = start + gt + 1;
    let open_tag = &lower[start..inner_start];
    if open_tag.ends_with("/>") {
        return Some(String::new());
    }
    take_until_close(html, &lower, inner_start, tag)
}

fn extract_by_class(html: &str, classes: &[&str]) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find('<') {
        let start = from + rel;
        if lower[start..].starts_with("</") {
            from = start + 2;
            continue;
        }
        let Some(gt) = lower[start..].find('>') else {
            break;
        };
        let tag_end = start + gt + 1;
        let open_l = &lower[start..tag_end];
        if open_l.starts_with("<!") || open_l.starts_with("<?") {
            from = tag_end;
            continue;
        }
        if class_matches(open_l, classes) {
            let tag = tag_name(open_l)?;
            if open_l.ends_with("/>") {
                from = tag_end;
                continue;
            }
            if let Some(inner) = take_until_close(html, &lower, tag_end, tag) {
                return Some(inner);
            }
        }
        from = tag_end;
    }
    None
}

fn class_matches(open_lower: &str, classes: &[&str]) -> bool {
    let Some(class_attr) = attr_value(open_lower, "class") else {
        return false;
    };
    class_attr
        .split_whitespace()
        .any(|token| classes.contains(&token))
}

fn attr_value(open_lower: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let idx = open_lower.find(&needle)?;
    let rest = &open_lower[idx + needle.len()..];
    let quote = rest.as_bytes().first().copied();
    if quote == Some(b'"') || quote == Some(b'\'') {
        let q = quote? as char;
        let end = rest[1..].find(q)?;
        Some(rest[1..1 + end].to_string())
    } else {
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn tag_name(open_lower: &str) -> Option<&str> {
    let rest = open_lower.strip_prefix('<')?;
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn take_until_close(orig: &str, lower: &str, inner_start: usize, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut depth = 1usize;
    let mut i = inner_start;
    while i < lower.len() {
        let slice = &lower[i..];
        let next_open = slice.find(&open);
        let next_close = slice.find(&close);
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                let abs = i + o;
                if is_same_tag_open(&lower[abs..], tag) {
                    depth += 1;
                }
                i = abs + open.len();
            }
            (Some(o), None) => {
                let abs = i + o;
                if is_same_tag_open(&lower[abs..], tag) {
                    depth += 1;
                }
                i = abs + open.len();
            }
            (_, Some(c)) => {
                let abs = i + c;
                depth -= 1;
                if depth == 0 {
                    return Some(orig[inner_start..abs].to_string());
                }
                i = abs + close.len();
            }
            (None, None) => break,
        }
    }
    None
}

fn is_same_tag_open(from_lower: &str, tag: &str) -> bool {
    let rest = match from_lower.strip_prefix('<') {
        Some(r) => r,
        None => return false,
    };
    if !rest.starts_with(tag) {
        return false;
    }
    matches!(
        rest.as_bytes().get(tag.len()),
        Some(b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'>') | None
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_landing_hosts() {
        assert!(is_paper_landing_url(Some(
            "https://arxiv.org/abs/1706.03762"
        )));
        assert!(is_paper_landing_url(Some(
            "https://rss.arxiv.org/abs/1706.03762"
        )));
        assert!(is_paper_landing_url(Some("https://doi.org/10.1/xyz")));
        assert!(!is_paper_landing_url(Some(
            "https://example.com/geometry-of-truth"
        )));
    }

    #[test]
    fn extracts_doi_from_article_metadata() {
        // Shape of a nature.com article page (truncated to the metas that matter).
        let nature = r#"<html><head>
<meta name="citation_doi" content="10.1038/s41467-026-76837-1">
<meta name="prism.doi" content="doi:10.1038/s41467-026-76837-1">
<meta name="dc.identifier" content="doi:10.1038/s41467-026-76837-1">
</head><body><article><p>Body text long enough to matter.</p></article></body></html>"#;
        assert_eq!(
            extract_paper_doi(nature).as_deref(),
            Some("10.1038/s41467-026-76837-1")
        );
        // prism.doi / dc.identifier carry a `doi:` prefix that gets stripped.
        let prism_only = r#"<meta name="prism.doi" content="doi:10.1038/s41592-026-03201-y">"#;
        assert_eq!(
            extract_paper_doi(prism_only).as_deref(),
            Some("10.1038/s41592-026-03201-y")
        );
        // Blog pages without paper metadata stay non-papers.
        assert_eq!(extract_paper_doi("<html><head></head></html>"), None);
        // Garbage content that is not a DOI is ignored.
        let junk = r#"<meta name="citation_doi" content="not-a-doi">"#;
        assert_eq!(extract_paper_doi(junk), None);
    }

    #[test]
    fn extracts_article_and_converts_headings() {
        let html = r#"<html><head><script>alert(1)</script></head>
        <body>
          <nav>Home</nav>
          <article>
            <h2>The Setup</h2>
            <p>We study a map $m$ from inputs to labels.</p>
          </article>
          <footer>subscribe</footer>
        </body></html>"#;
        let inner = extract_article_html(html);
        assert!(inner.contains("The Setup"), "{inner}");
        assert!(!inner.to_ascii_lowercase().contains("<nav>"));
        let md = html_to_markdown(&inner);
        assert!(md.contains("The Setup"), "{md}");
        assert!(md.contains("$m$"), "{md}");
    }

    #[test]
    fn extracts_class_hint() {
        let html = r#"<div class="site"><div class="entry-content"><p>Full post body here with enough text to pass the threshold.</p></div></div>"#;
        let inner = extract_article_html(html);
        assert!(inner.contains("Full post body"), "{inner}");
    }

    #[test]
    fn strips_trailing_ellipsis() {
        assert_eq!(strip_trailing_ellipsis("Hello world [...]"), "Hello world");
        assert_eq!(strip_trailing_ellipsis("Hello world…"), "Hello world");
        assert_eq!(
            strip_trailing_ellipsis("Complete sentence."),
            "Complete sentence."
        );
    }

    #[test]
    fn prefixes_markdown_title() {
        assert_eq!(
            ensure_heading("## The Setup\n\nHello [...]", "The Geometry of Truth"),
            "# The Geometry of Truth\n\n## The Setup\n\nHello"
        );
        assert_eq!(
            ensure_heading(
                "# The Geometry of Truth\n\n## The Setup",
                "The Geometry of Truth"
            ),
            "# The Geometry of Truth\n\n## The Setup"
        );
    }

    #[test]
    fn readability_picks_content_over_sidebar() {
        let html = r#"
        <html><body>
          <div class="sidebar">Short sidebar text that is not the main content at all.</div>
          <div class="article-content">
            <p>This is the real article content with enough length to be considered meaningful by the scoring algorithm that evaluates candidates.</p>
          </div>
        </body></html>"#;
        let inner = extract_article_html(html);
        assert!(inner.contains("real article content"), "{inner}");
        assert!(!inner.contains("sidebar"), "{inner}");
    }

    #[test]
    fn readability_prefers_specific_inner_container() {
        let html = r#"
        <html><body>
          <article>
            <div class="post-content">
              <p>Inner content that should be selected over the outer article wrapper because it has a positive class hint bonus.</p>
            </div>
          </article>
        </body></html>"#;
        let inner = extract_article_html(html);
        assert!(inner.contains("Inner content"), "{inner}");
        assert!(!inner.contains("<article"), "{inner}");
    }

    #[test]
    fn readability_penalizes_link_heavy_containers() {
        let html = r##"
        <html><body>
          <div class="nav-links">
            <a href="#">Link 1</a>
            <a href="#">Link 2</a>
            <a href="#">Link 3</a>
            <a href="#">Link 4</a>
            <a href="#">Link 5</a>
            <a href="#">Link 6</a>
            <a href="#">Link 7</a>
            <a href="#">Link 8</a>
          </div>
          <div class="content">
            <p>This is actual readable content that has meaningful paragraphs and very few links in comparison to the navigation area.</p>
          </div>
        </body></html>"##;
        let inner = extract_article_html(html);
        assert!(inner.contains("actual readable content"), "{inner}");
    }

    #[test]
    fn preserves_latex_inline_math() {
        let html = r#"<div><p>The function $f(x) = x^2$ maps reals to reals.</p></div>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("$f(x) = x^2$"), "{md}");
    }

    #[test]
    fn preserves_latex_block_math() {
        let html = r#"<div><p>Before.</p><p>\begin{equation}E = mc^2\end{equation}</p><p>After.</p></div>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("$$ E = mc^2 $$"), "{md}");
        assert!(!md.contains("\\begin{equation}"), "{md}");
    }

    #[test]
    fn keeps_katex_supported_env_wrappers() {
        let html = r#"<p>\begin{aligned}a &= b \\ c &= d\end{aligned}</p>"#;
        let md = html_to_markdown(html);
        assert!(
            md.contains("$$ \\begin{aligned}a &= b \\\\ c &= d\\end{aligned} $$"),
            "{md}"
        );
    }

    #[test]
    fn strips_html_artifacts_inside_math() {
        let html = "<p>\\begin{equation}\\begin{aligned}<br />\na =&\\, b \\\\<br />\n=&\\, c &lt; d<br />\n\\end{aligned}\\end{equation}</p>";
        let md = html_to_markdown(html);
        assert!(
            md.contains("$$ \\begin{aligned} a =&\\, b \\\\ =&\\, c < d \\end{aligned} $$"),
            "{md}"
        );
        assert!(!md.contains("<br"), "{md}");
    }

    #[test]
    fn preserves_latex_backslash_commands() {
        let html = r#"<div><p>We use $\boldsymbol{\alpha} + \frac{1}{2}$ here.</p></div>"#;
        let md = html_to_markdown(html);
        assert!(
            md.contains("$\\boldsymbol{\\alpha} + \\frac{1}{2}$"),
            "{md}"
        );
    }
}
