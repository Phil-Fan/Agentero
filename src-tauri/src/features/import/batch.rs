use std::collections::HashMap;
use std::path::Path;

use crate::features::catalog::papers;

use super::parse::{self, extract_primary_identifier, IdentifierKind, SkillSource};
use super::{identifier_kind_column, identifier_kind_str, SkippedImport};

pub(crate) enum SkillBatchMode {
    Collect,
    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    RejectRemote,
}

pub(crate) struct PendingIdentifierImport {
    pub raw: String,
}

pub(crate) struct PendingSkillImport {
    pub raw: String,
    pub source: SkillSource,
}

pub(crate) struct IdentifierBatchPreflight {
    pub papers: Vec<PendingIdentifierImport>,
    pub skills: Vec<PendingSkillImport>,
    /// Free text with no recognizable identifier → title/keyword search.
    pub queries: Vec<String>,
    pub skipped: Vec<SkippedImport>,
    pub errors: Vec<String>,
}

pub(crate) fn preflight_identifier_batch(
    texts: &[String],
    catalog_root: &Path,
    skill_mode: SkillBatchMode,
    remote_catalog: bool,
) -> IdentifierBatchPreflight {
    let mut papers = Vec::new();
    let mut skills = Vec::new();
    let mut queries = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();

    for input in texts {
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        let units = match classify_segment(input) {
            Segment::Identifiers(units) => units,
            Segment::Query(query) => {
                queries.push(query);
                continue;
            }
        };

        for (raw, kind, value) in units {
            let raw = raw.as_str();
            if kind == IdentifierKind::Skill && matches!(skill_mode, SkillBatchMode::RejectRemote) {
                errors.push(format!(
                    "{raw}: skill import is not supported for remote vaults"
                ));
                continue;
            }

            let kind_str = identifier_kind_str(kind);
            let dedup_key = format!("{kind_str}:{value}");
            if seen.contains_key(&dedup_key) {
                skipped.push(SkippedImport {
                    raw: raw.to_string(),
                    kind: kind_str,
                    value: value.clone(),
                    reason: "duplicate_in_batch".to_string(),
                });
                continue;
            }
            seen.insert(dedup_key, raw.to_string());

            if kind == IdentifierKind::Skill {
                let Some(source) = parse::extract_skill_source(raw) else {
                    errors.push(format!("{raw}: invalid skill source"));
                    continue;
                };
                skills.push(PendingSkillImport {
                    raw: raw.to_string(),
                    source,
                });
                continue;
            }

            if let Some(column) = identifier_kind_column(kind) {
                match papers::find_by_identifier(catalog_root, column, &value) {
                    Ok(Some(_record)) => {
                        skipped.push(SkippedImport {
                            raw: raw.to_string(),
                            kind: kind_str,
                            value: value.clone(),
                            reason: "already_in_library".to_string(),
                        });
                        continue;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if remote_catalog {
                            log::warn!("remote catalog lookup failed for {value}: {e}");
                        } else {
                            log::warn!("catalog lookup failed for {value}: {e}");
                        }
                    }
                }
            }

            papers.push(PendingIdentifierImport {
                raw: raw.to_string(),
            });
        }
    }

    IdentifierBatchPreflight {
        papers,
        skills,
        queries,
        skipped,
        errors,
    }
}

enum Segment {
    Identifiers(Vec<(String, IdentifierKind, String)>),
    Query(String),
}

/// Classify one input segment. Space-separated identifier lists still expand
/// (`"1706.03762 10.1038/…"`); anything else with no identifier is free text.
///
/// Only single tokens are matched as a whole: `clean_doi` / `clean_isbn` scan
/// the entire string, so a whole-segment match on multi-word input would
/// swallow the rest of a list or a title.
fn classify_segment(input: &str) -> Segment {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() <= 1 {
        return match extract_primary_identifier(input) {
            Some((kind, value)) => Segment::Identifiers(vec![(input.to_string(), kind, value)]),
            None => Segment::Query(input.to_string()),
        };
    }

    // Skill sources (`npx skills add …`) are the only identifiers with spaces.
    if let Some(source) = parse::extract_skill_source(input) {
        return Segment::Identifiers(vec![(
            input.to_string(),
            IdentifierKind::Skill,
            source.source,
        )]);
    }

    let mut units = Vec::with_capacity(tokens.len());
    for token in &tokens {
        let Some((kind, value)) = extract_primary_identifier(token) else {
            return Segment::Query(input.to_string());
        };
        units.push((token.to_string(), kind, value));
    }
    Segment::Identifiers(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Option<Vec<IdentifierKind>> {
        match classify_segment(input) {
            Segment::Identifiers(units) => Some(units.into_iter().map(|(_, k, _)| k).collect()),
            Segment::Query(_) => None,
        }
    }

    #[test]
    fn keeps_multi_word_skill_command_intact() {
        assert_eq!(
            kinds("npx skills add anthropics/skills --skill pptx"),
            Some(vec![IdentifierKind::Skill])
        );
    }

    #[test]
    fn expands_space_separated_identifiers() {
        assert_eq!(
            kinds("1706.03762 10.1038/nature12373"),
            Some(vec![IdentifierKind::Arxiv, IdentifierKind::Doi])
        );
    }

    #[test]
    fn treats_free_text_as_query() {
        assert!(matches!(
            classify_segment("Attention is all you need"),
            Segment::Query(q) if q == "Attention is all you need"
        ));
        assert!(matches!(classify_segment("AlphaFold"), Segment::Query(_)));
    }

    #[test]
    fn does_not_swallow_a_title_containing_a_doi_fragment() {
        assert!(matches!(
            classify_segment("Revisiting 10.1038/nature12373 and friends"),
            Segment::Query(_)
        ));
    }
}
