//! Turn RSS excerpts / article HTML into Markdown for the plaza detail page.

use super::parse::strip_html;
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

pub fn html_to_markdown(html: &str) -> String {
    htmd::convert(html)
        .unwrap_or_else(|_| strip_html(html))
        .replace('\u{200b}', "")
        .trim()
        .to_string()
}

pub fn looks_truncated(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    lower.ends_with("[...]")
        || t.ends_with("[…]")
        || t.ends_with("...")
        || t.ends_with('…')
        || lower.ends_with("[..]")
        || lower.ends_with("read more")
        || lower.ends_with("continue reading")
}

/// Paper landing pages (arXiv abs / DOI) are not useful article HTML.
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

/// Prefer `<article>` / common post containers, then `<main>` / `<body>`.
pub fn extract_article_html(html: &str) -> String {
    let cleaned = strip_noise_tags(html);
    if let Some(inner) = extract_element(&cleaned, "article") {
        if strip_html(&inner).len() > 20 {
            return inner;
        }
    }
    if let Some(inner) = extract_by_class(&cleaned, ARTICLE_CLASS_HINTS) {
        if strip_html(&inner).len() > 40 {
            return inner;
        }
    }
    if let Some(inner) = extract_element(&cleaned, "main") {
        if strip_html(&inner).len() > 40 {
            return inner;
        }
    }
    extract_element(&cleaned, "body").unwrap_or(cleaned)
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
    fn detects_truncation() {
        assert!(looks_truncated("Hello world [...]"));
        assert!(looks_truncated("Hello world…"));
        assert!(!looks_truncated("A complete abstract about $m$ tokens."));
    }

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
}
