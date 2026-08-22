//! Identifier extraction (subset of Zotero extractIdentifiers).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierKind {
    Doi,
    Isbn,
    Arxiv,
    Pmid,
    AdsBibcode,
    Url,
    Skill,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillSource {
    pub owner: String,
    pub repo: String,
    pub reference: Option<String>,
    pub subpath: Option<String>,
    pub skill_names: Vec<String>,
    pub source: String,
}

/// Returns the first recognized identifier in `text`.
pub fn extract_primary_identifier(text: &str) -> Option<(IdentifierKind, String)> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }

    if let Some(source) = extract_skill_source(t) {
        return Some((IdentifierKind::Skill, source.source));
    }

    if t.starts_with("http://") || t.starts_with("https://") {
        return Some((IdentifierKind::Url, t.to_string()));
    }

    if let Some(doi) = clean_doi(t) {
        return Some((IdentifierKind::Doi, doi));
    }
    if let Some(id) = extract_arxiv_id(t) {
        return Some((IdentifierKind::Arxiv, id));
    }
    if let Some(isbn) = clean_isbn(t) {
        return Some((IdentifierKind::Isbn, isbn));
    }
    // PMID: 1–9 digits
    if let Some(caps) = regex_pmid(t) {
        return Some((IdentifierKind::Pmid, caps));
    }
    if let Some(ads) = regex_ads(t) {
        return Some((IdentifierKind::AdsBibcode, ads));
    }
    None
}

pub fn extract_skill_source(text: &str) -> Option<SkillSource> {
    let input = text.trim();
    if input.is_empty() {
        return None;
    }

    if input.starts_with("npx skills add ") {
        let mut words = input.split_whitespace();
        words.next();
        words.next();
        words.next();
        let source = words.next()?;
        let mut parsed = parse_skill_repo(source, input)?;
        let mut names = Vec::new();
        let mut iter = words.peekable();
        while let Some(word) = iter.next() {
            if word == "--skill" || word == "-s" {
                if let Some(name) = iter.next() {
                    names.push(name.to_string());
                }
            } else if let Some(name) = word.strip_prefix("--skill=") {
                names.push(name.to_string());
            }
        }
        parsed.skill_names = names;
        return Some(parsed);
    }

    if let Some(rest) = input.strip_prefix("github:") {
        return parse_skill_repo(rest, input);
    }

    let url = url::Url::parse(input).ok()?;
    match url.host_str()? {
        "github.com" | "www.github.com" => parse_github_url(&url, input),
        "skills.sh" | "www.skills.sh" => {
            let parts: Vec<_> = url.path_segments()?.filter(|p| !p.is_empty()).collect();
            if parts.len() < 3 {
                return None;
            }
            Some(SkillSource {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                reference: fragment_ref(&url),
                subpath: None,
                skill_names: vec![parts[2].to_string()],
                source: input.to_string(),
            })
        }
        _ => None,
    }
}

fn parse_skill_repo(value: &str, source: &str) -> Option<SkillSource> {
    let mut raw = value.trim().trim_end_matches('/');
    if raw.starts_with("http://") || raw.starts_with("https://") {
        let url = url::Url::parse(raw).ok()?;
        return parse_github_url(&url, source);
    }
    let mut reference = None;
    if let Some((base, fragment)) = raw.split_once('#') {
        raw = base;
        if !fragment.is_empty() {
            reference = Some(fragment.to_string());
        }
    }
    let raw = raw.strip_suffix(".git").unwrap_or(raw);
    let parts: Vec<_> = raw.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 || !valid_github_part(parts[0]) || !valid_github_part(parts[1]) {
        return None;
    }
    Some(SkillSource {
        owner: parts[0].to_string(),
        repo: parts[1].to_string(),
        reference,
        subpath: None,
        skill_names: Vec::new(),
        source: source.to_string(),
    })
}

fn parse_github_url(url: &url::Url, source: &str) -> Option<SkillSource> {
    let parts: Vec<_> = url.path_segments()?.filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 || !valid_github_part(parts[0]) || !valid_github_part(parts[1]) {
        return None;
    }
    let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    let (reference, subpath) = if parts.get(2) == Some(&"tree") && parts.len() >= 4 {
        let reference = Some(parts[3].to_string());
        let subpath = if parts.len() > 4 {
            Some(parts[4..].join("/"))
        } else {
            None
        };
        (reference, subpath)
    } else {
        (fragment_ref(url), None)
    };
    Some(SkillSource {
        owner: parts[0].to_string(),
        repo: repo.to_string(),
        reference,
        subpath,
        skill_names: Vec::new(),
        source: source.to_string(),
    })
}

fn fragment_ref(url: &url::Url) -> Option<String> {
    url.fragment().filter(|s| !s.is_empty()).map(str::to_string)
}

fn valid_github_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn extract_arxiv_id(text: &str) -> Option<String> {
    let s = text.trim();
    // URL form
    let stripped = s
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("export.arxiv.org/")
        .trim_start_matches("arxiv.org/");
    let after_path = if let Some(rest) = stripped.strip_prefix("abs/") {
        rest
    } else if let Some(rest) = stripped.strip_prefix("pdf/") {
        rest
    } else if let Some(rest) = stripped.strip_prefix("html/") {
        rest
    } else if let Some(rest) = stripped.strip_prefix("src/") {
        rest
    } else if let Some(rest) = stripped.strip_prefix("e-print/") {
        rest
    } else {
        s.trim_start_matches("arXiv:").trim_start_matches("arxiv:")
    };
    let id = after_path
        .split(['?', '#', '/'])
        .next()
        .unwrap_or(after_path)
        .trim_end_matches(".pdf");
    let id = id.trim().trim_end_matches('/');
    // new-style 1706.03762 or old-style hep-th/9901001
    let bare = strip_arxiv_version(id);
    if is_arxiv_id(&bare) {
        Some(bare)
    } else {
        None
    }
}

fn is_arxiv_id(s: &str) -> bool {
    // 1234.5678 or 1234.56789
    if s.len() >= 9 && s.as_bytes().get(4) == Some(&b'.') {
        let (a, b) = s.split_at(4);
        let b = &b[1..];
        return a.chars().all(|c| c.is_ascii_digit())
            && (b.len() == 4 || b.len() == 5)
            && b.chars().all(|c| c.is_ascii_digit());
    }
    // archive/YYMMNNN
    if let Some((arch, num)) = s.split_once('/') {
        return !arch.is_empty() && num.len() == 7 && num.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// Canonical arXiv id normalizer: trim, drop an `arXiv:` / `arxiv:` prefix
/// (re-trimming any whitespace behind the colon), then strip a trailing `vN`
/// version suffix (only when the `v` is not the first character, so ids like
/// `cs.CL/0101001` survive). Single shared implementation for import and
/// coolpapers; supersedes the former local copies in `assets.rs`, `mod.rs`
/// and `coolpapers/mod.rs`.
pub(crate) fn strip_arxiv_version(id: &str) -> String {
    let s = id
        .trim()
        .trim_start_matches("arXiv:")
        .trim_start_matches("arxiv:")
        .trim();
    if let Some(i) = s.rfind('v') {
        if s[i + 1..].chars().all(|c| c.is_ascii_digit()) && i > 0 {
            return s[..i].to_string();
        }
    }
    s.to_string()
}

pub(crate) fn clean_doi(s: &str) -> Option<String> {
    let mut x = s.trim().to_string();
    if x.starts_with("https://doi.org/") {
        x = x["https://doi.org/".len()..].to_string();
    } else if x.starts_with("http://doi.org/") {
        x = x["http://doi.org/".len()..].to_string();
    } else if x.starts_with("doi:") {
        x = x["doi:".len()..].trim().to_string();
    }
    // 10.xxxx/...
    if let Some(start) = x.find("10.") {
        let cand = &x[start..];
        let end = cand
            .find(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .unwrap_or(cand.len());
        let doi = cand[..end].trim_end_matches(['.', ',', ')']).to_string();
        if doi.contains('/') {
            return Some(doi);
        }
    }
    None
}

fn clean_isbn(s: &str) -> Option<String> {
    let digits: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .collect();
    let upper = digits.to_uppercase();
    if upper.len() == 13 && (upper.starts_with("978") || upper.starts_with("979")) {
        return Some(upper);
    }
    if upper.len() == 10 {
        return Some(upper);
    }
    None
}

fn regex_pmid(s: &str) -> Option<String> {
    let t = s
        .trim()
        .trim_start_matches("PMID:")
        .trim_start_matches("pmid:");
    let t = t.trim();
    if !t.is_empty() && t.len() <= 9 && t.chars().all(|c| c.is_ascii_digit()) {
        return Some(t.to_string());
    }
    None
}

fn regex_ads(s: &str) -> Option<String> {
    // 2015ApJ...810...89S — 19 chars-ish
    let t = s.trim();
    if t.len() == 19
        && t.as_bytes()[0].is_ascii_digit()
        && t.as_bytes()[1].is_ascii_digit()
        && t.as_bytes()[2].is_ascii_digit()
        && t.as_bytes()[3].is_ascii_digit()
    {
        return Some(t.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arxiv_id_new() {
        assert_eq!(
            extract_arxiv_id("1706.03762").as_deref(),
            Some("1706.03762")
        );
        assert_eq!(
            extract_arxiv_id("https://arxiv.org/abs/1706.03762v1").as_deref(),
            Some("1706.03762")
        );
        assert_eq!(
            extract_arxiv_id("https://arxiv.org/pdf/2508.05004.pdf?download=1").as_deref(),
            Some("2508.05004")
        );
    }

    #[test]
    fn strip_arxiv_version_normalizes_prefix_and_suffix() {
        assert_eq!(strip_arxiv_version("2608.13558v3"), "2608.13558");
        assert_eq!(strip_arxiv_version("arXiv:1706.03762v2"), "1706.03762");
        assert_eq!(strip_arxiv_version("arxiv:1706.03762"), "1706.03762");
        assert_eq!(strip_arxiv_version("  1706.03762  "), "1706.03762");
        // Old-style id without a version stays intact.
        assert_eq!(strip_arxiv_version("hep-th/9901001"), "hep-th/9901001");
        // A leading `v` is never treated as a version marker.
        assert_eq!(strip_arxiv_version("v2"), "v2");
    }

    #[test]
    fn arxiv_id_with_prefix_and_version() {
        // The old parse.rs-local strip_version missed the `arXiv:` prefix and
        // returned None here; the canonical stripper resolves it.
        assert_eq!(
            extract_arxiv_id("arXiv:1706.03762v2").as_deref(),
            Some("1706.03762")
        );
        assert_eq!(
            extract_arxiv_id("https://arxiv.org/abs/arXiv:2608.13558v1").as_deref(),
            Some("2608.13558")
        );
    }

    #[test]
    fn doi_clean() {
        assert_eq!(
            clean_doi("https://doi.org/10.1038/nature12373").as_deref(),
            Some("10.1038/nature12373")
        );
    }

    #[test]
    fn parses_skill_sources() {
        let source =
            extract_skill_source("npx skills add vercel-labs/agent-skills --skill frontend-design")
                .unwrap();
        assert_eq!(source.owner, "vercel-labs");
        assert_eq!(source.repo, "agent-skills");
        assert_eq!(source.skill_names, vec!["frontend-design"]);

        let source =
            extract_skill_source("https://github.com/openai/skills/tree/main/skills/create-plan")
                .unwrap();
        assert_eq!(source.reference.as_deref(), Some("main"));
        assert_eq!(source.subpath.as_deref(), Some("skills/create-plan"));

        assert!(extract_skill_source("https://example.com/paper").is_none());
    }

    #[test]
    fn parses_requested_skill_import_examples() {
        for input in [
            "https://github.com/mattpocock/skills",
            "https://github.com/alchaincyf/nuwa-skill",
        ] {
            let (kind, value) = extract_primary_identifier(input).unwrap();
            assert_eq!(kind, IdentifierKind::Skill);
            assert_eq!(value, input);
        }

        let input = "npx skills add https://github.com/anthropics/skills --skill pptx";
        let (kind, value) = extract_primary_identifier(input).unwrap();
        assert_eq!(kind, IdentifierKind::Skill);
        assert_eq!(value, input);
        let source = extract_skill_source(input).unwrap();
        assert_eq!(source.owner, "anthropics");
        assert_eq!(source.repo, "skills");
        assert_eq!(source.skill_names, vec!["pptx"]);
    }
}
