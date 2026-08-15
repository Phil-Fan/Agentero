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

pub fn strip_leading_title(md: &str, title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        return md.to_string();
    }
    let mut lines = md.lines();
    let Some(first) = lines.next() else {
        return md.to_string();
    };
    let heading = first.trim().trim_start_matches('#').trim();
    if heading.eq_ignore_ascii_case(title) {
        return lines.collect::<Vec<_>>().join("\n").trim().to_string();
    }
    md.to_string()
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
    fn strips_duplicate_title() {
        let md = "# The Geometry of Truth\n\n## The Setup\n\nHello.";
        assert_eq!(
            strip_leading_title(md, "The Geometry of Truth"),
            "## The Setup\n\nHello."
        );
    }
}
