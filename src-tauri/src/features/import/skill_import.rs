#[cfg(not(feature = "desktop"))]
use super::AppHandle;
use crate::core::error::AppError;
use crate::features::import::assets::{extract_tar_safe, http_get_bytes_with_progress};
use crate::features::import::parse::SkillSource;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::AppHandle;
use walkdir::WalkDir;

const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_EXTRACTED_FILES: usize = 2_000;
const MAX_SKILL_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportResult {
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: String,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCandidate {
    pub name: String,
    pub description: String,
    pub source: String,
    pub relative_path: String,
    pub already_installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDiscovery {
    pub discovery_id: String,
    pub source: String,
    pub candidates: Vec<SkillCandidate>,
}

#[derive(Debug, Clone)]
struct ParsedSkillCandidate {
    dir: PathBuf,
    name: String,
    description: String,
}

pub async fn discover_skill_source(
    vault: &Path,
    source: &SkillSource,
    app: Option<&AppHandle>,
    task_id: Option<&str>,
) -> Result<SkillDiscovery, AppError> {
    let reference = match &source.reference {
        Some(reference) => reference.clone(),
        None => default_branch(&source.owner, &source.repo).await?,
    };
    let archive_url = format!(
        "https://codeload.github.com/{}/{}/tar.gz/{}",
        source.owner,
        source.repo,
        urlencoding::encode(&reference)
    );
    let archive = http_get_bytes_with_progress(
        &archive_url,
        Duration::from_secs(120),
        app,
        task_id,
        None,
        "skill",
    )
    .await?;
    if archive.len() > MAX_ARCHIVE_BYTES {
        return Err(AppError::message("skill archive is too large"));
    }

    let discovery_id = uuid::Uuid::new_v4().to_string();
    let temp = discovery_dir(&discovery_id)?;
    fs::create_dir_all(&temp)?;
    fs::write(temp.join("archive.tar.gz"), &archive)?;
    fs::write(
        temp.join("source.json"),
        serde_json::to_vec(&serde_json::json!({
            "source": source,
            "reference": reference,
        }))?,
    )?;
    let candidates = discover_candidates(&temp, vault, source)?;
    Ok(SkillDiscovery {
        discovery_id,
        source: source.source.clone(),
        candidates,
    })
}

async fn default_branch(owner: &str, repo: &str) -> Result<String, AppError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let client = crate::features::network::client_builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Agentero/skill-import")
        .build()
        .map_err(|e| AppError::message(format!("http client: {e}")))?;
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::message(format!("skill metadata request: {e}")))?;
    if !response.status().is_success() {
        return Err(AppError::message(format!(
            "GitHub repository lookup failed: {}",
            response.status()
        )));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::message(format!("invalid GitHub response: {e}")))?;
    body.get("default_branch")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::message("GitHub response did not include a default branch"))
}

pub fn install_discovered_skills(
    vault: &Path,
    discovery_id: &str,
    selected_names: &[String],
) -> Result<Vec<SkillImportResult>, AppError> {
    if !vault.is_dir() {
        return Err(AppError::message("vault path is not a directory"));
    }
    let temp = discovery_dir(discovery_id)?;
    let archive = fs::read(temp.join("archive.tar.gz"))?;
    let metadata: serde_json::Value = serde_json::from_slice(&fs::read(temp.join("source.json"))?)?;
    let source: SkillSource = serde_json::from_value(
        metadata
            .get("source")
            .cloned()
            .ok_or_else(|| AppError::message("skill discovery metadata is invalid"))?,
    )?;
    let reference = metadata
        .get("reference")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AppError::message("skill discovery reference is missing"))?;
    let result = install_from_archive(&temp, vault, &source, reference, &archive, selected_names);
    let _ = fs::remove_dir_all(&temp);
    result
}

pub fn discard_skill_discovery(discovery_id: &str) -> Result<(), AppError> {
    let temp = discovery_dir(discovery_id)?;
    if temp.exists() {
        fs::remove_dir_all(temp)?;
    }
    Ok(())
}

fn install_from_archive(
    temp: &Path,
    vault: &Path,
    source: &SkillSource,
    reference: &str,
    archive: &[u8],
    selected_names: &[String],
) -> Result<Vec<SkillImportResult>, AppError> {
    let tar_bytes = decode_gzip(archive)?;
    if tar_bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(AppError::message("unpacked skill archive is too large"));
    }
    extract_tar_safe(temp, &tar_bytes)?;

    let candidates: Vec<_> = discover_candidates_from_tar(temp, source, &tar_bytes)?;
    let candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            selected_names.is_empty()
                || selected_names
                    .iter()
                    .any(|name| name == "*" || name == &candidate.name)
        })
        .collect();
    if candidates.is_empty() {
        return Err(AppError::message("no selected Skill remains to install"));
    }

    let skills_root = vault.join(".agents/skills");
    fs::create_dir_all(&skills_root)?;
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let target = skills_root.join(&candidate.name);
        let relative_path = format!(".agents/skills/{}", candidate.name);
        if target.exists() {
            results.push(SkillImportResult {
                name: candidate.name,
                description: candidate.description,
                path: relative_path,
                source: source.source.clone(),
                skipped: true,
            });
            continue;
        }
        copy_dir(&candidate.dir, &target)?;
        let provenance = serde_json::json!({
            "source": source.source,
            "owner": source.owner,
            "repo": source.repo,
            "reference": reference,
            "installedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        });
        fs::write(
            target.join("agentero-skill.json"),
            serde_json::to_vec_pretty(&provenance)?,
        )?;
        results.push(SkillImportResult {
            name: candidate.name,
            description: candidate.description,
            path: relative_path,
            source: source.source.clone(),
            skipped: false,
        });
    }
    Ok(results)
}

fn discover_candidates(
    temp: &Path,
    vault: &Path,
    source: &SkillSource,
) -> Result<Vec<SkillCandidate>, AppError> {
    let archive = fs::read(temp.join("archive.tar.gz"))?;
    let tar_bytes = decode_gzip(&archive)?;
    discover_candidates_from_tar(temp, source, &tar_bytes).map(|candidates| {
        candidates
            .into_iter()
            .map(|candidate| {
                let already_installed = vault.join(".agents/skills").join(&candidate.name).is_dir();
                SkillCandidate {
                    name: candidate.name,
                    description: candidate.description,
                    source: source.source.clone(),
                    relative_path: candidate
                        .dir
                        .strip_prefix(temp)
                        .unwrap_or(&candidate.dir)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    already_installed,
                }
            })
            .collect()
    })
}

fn discover_candidates_from_tar(
    temp: &Path,
    source: &SkillSource,
    tar_bytes: &[u8],
) -> Result<Vec<ParsedSkillCandidate>, AppError> {
    extract_tar_safe(temp, tar_bytes)?;
    let candidates: Vec<_> = WalkDir::new(temp)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == "SKILL.md")
        .take(MAX_EXTRACTED_FILES + 1)
        .map(|entry| {
            let dir = entry.path().parent().unwrap_or(temp).to_path_buf();
            let content = fs::read_to_string(entry.path())?;
            let (name, description) = parse_skill_metadata(&content)?;
            Ok(ParsedSkillCandidate {
                dir,
                name,
                description,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    if candidates.len() > MAX_EXTRACTED_FILES {
        return Err(AppError::message("skill archive contains too many files"));
    }

    Ok(candidates
        .into_iter()
        .filter(|candidate| {
            let relative = candidate
                .dir
                .strip_prefix(temp)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let path_matches = source.subpath.as_deref().is_none_or(|subpath| {
                relative == subpath || relative.ends_with(&format!("/{subpath}"))
            });
            let name_matches = source.skill_names.is_empty()
                || source
                    .skill_names
                    .iter()
                    .any(|name| name == "*" || name == &candidate.name);
            path_matches && name_matches
        })
        .collect())
}

fn discovery_dir(discovery_id: &str) -> Result<PathBuf, AppError> {
    let id = uuid::Uuid::parse_str(discovery_id)
        .map_err(|_| AppError::message("invalid skill discovery id"))?;
    Ok(std::env::temp_dir().join(format!("agentero-skill-discovery-{id}")))
}

fn decode_gzip(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut decoder = GzDecoder::new(bytes);
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|e| AppError::message(format!("skill archive gzip: {e}")))?;
    Ok(output)
}

fn parse_skill_metadata(content: &str) -> Result<(String, String), AppError> {
    let rest = content
        .strip_prefix("---\n")
        .ok_or_else(|| AppError::message("SKILL.md is missing YAML frontmatter"))?;
    let (frontmatter, _) = rest
        .split_once("\n---")
        .ok_or_else(|| AppError::message("SKILL.md has invalid YAML frontmatter"))?;
    let mut name = None;
    let mut description = String::new();
    for line in frontmatter.lines() {
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(value.trim().trim_matches(['"', '\'']).to_string());
        } else if let Some(value) = line.strip_prefix("description:") {
            description = value.trim().trim_matches(['"', '\'']).to_string();
        }
    }
    let name = name.filter(|name| valid_skill_name(name)).ok_or_else(|| {
        AppError::message(
            "SKILL.md has an invalid name; use lowercase letters, numbers, and hyphens",
        )
    })?;
    if description.len() > MAX_DESCRIPTION_LEN {
        return Err(AppError::message("SKILL.md description is too long"));
    }
    Ok((name, description))
}

fn valid_skill_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SKILL_NAME_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), AppError> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| AppError::message(format!("skill path: {e}")))?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_skill_names() {
        assert!(valid_skill_name("frontend-design"));
        assert!(!valid_skill_name("Frontend Design"));
        assert!(!valid_skill_name("../escape"));
    }

    #[test]
    fn parses_frontmatter() {
        let (name, description) = parse_skill_metadata(
            "---\nname: example-skill\ndescription: Useful instructions\n---\n# Body",
        )
        .unwrap();
        assert_eq!(name, "example-skill");
        assert_eq!(description, "Useful instructions");
    }
}
