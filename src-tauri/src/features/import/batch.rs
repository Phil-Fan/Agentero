use std::collections::HashMap;
use std::path::Path;

use crate::features::catalog::papers;

use super::parse::{self, extract_primary_identifier, IdentifierKind, SkillSource};
use super::{identifier_kind_column, identifier_kind_str, SkippedImport};

pub(crate) enum SkillBatchMode {
    Collect,
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
    let mut skipped = Vec::new();
    let mut errors = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();

    for raw in texts {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let Some((kind, value)) = extract_primary_identifier(raw) else {
            errors.push(format!("{raw}: unrecognized identifier"));
            continue;
        };

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

    IdentifierBatchPreflight {
        papers,
        skills,
        skipped,
        errors,
    }
}
