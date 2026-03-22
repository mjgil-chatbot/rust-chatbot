use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use ai_history_core::{CheckpointId, ContentHash, ManifestId, SessionId, WorkspacePath};
use ai_history_snapshot::manifest::{FileEntry, Manifest};
use ai_history_snapshot::scanner::{Scanner, ScannerConfig};
use ai_history_store::{BlobStore, HistoryStore as SqliteStore};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::history_support::read_or_restore_history_object;

const PATCH_SCHEMA_VERSION: u32 = 1;
const PRIMARY_REPO_ROLE: &str = "workspace";
const COMPANION_REPO_ROLE: &str = "companion";
const PATCH_FORMAT: &str = "git-diff-binary-head-plus-untracked";
const DEFAULT_MAX_REPO_PATCH_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepoPatchCaptureState {
    #[default]
    Captured,
    OmittedOversize,
    OmittedQuota,
}

impl RepoPatchCaptureState {
    fn captured(self) -> bool {
        matches!(self, Self::Captured)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RepoPatchMetadata {
    schema_version: u32,
    repo_name: String,
    repo_root: String,
    role: String,
    base_commit: String,
    apply_commit: String,
    patch_format: String,
    tracked_patch_bytes: usize,
    untracked_file_count: usize,
    patch_is_empty: bool,
    #[serde(default)]
    total_patch_bytes: usize,
    #[serde(default)]
    capture_state: RepoPatchCaptureState,
    #[serde(default)]
    omitted_reason: Option<String>,
    captured_at: String,
}

#[derive(Clone, Debug)]
struct RepoPatchArtifact {
    slug: String,
    metadata: RepoPatchMetadata,
    patch: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CheckpointRepoSummary {
    pub(crate) checkpoint_id: CheckpointId,
    pub(crate) repos: Vec<RepoPatchSummary>,
}

#[derive(Clone, Debug)]
pub(crate) struct RepoPatchSummary {
    pub(crate) slug: String,
    pub(crate) repo_name: String,
    pub(crate) role: String,
    pub(crate) repo_root: String,
    pub(crate) base_commit: String,
    pub(crate) apply_commit: String,
    pub(crate) patch_bytes: usize,
    pub(crate) tracked_patch_bytes: usize,
    pub(crate) untracked_file_count: usize,
    pub(crate) patch_is_empty: bool,
    pub(crate) total_patch_bytes: usize,
    pub(crate) capture_state: RepoPatchCaptureState,
    pub(crate) omitted_reason: Option<String>,
    pub(crate) captured_at: String,
}

#[derive(Clone, Debug)]
pub(crate) struct MaterializedRepo {
    pub(crate) repo_name: String,
    pub(crate) slug: String,
    pub(crate) source_repo_root: String,
    pub(crate) output_repo_root: String,
    pub(crate) base_commit: String,
    pub(crate) apply_commit: String,
    pub(crate) patch_bytes: usize,
}

#[derive(Clone, Debug)]
struct RepoTarget {
    role: &'static str,
    root: PathBuf,
}

pub(crate) fn build_checkpoint_manifest(
    store: &mut SqliteStore,
    workspace_root: &Path,
    created_at: &str,
) -> Result<Manifest> {
    if let Some(manifest) = build_repo_patch_manifest(store, workspace_root, created_at)? {
        return Ok(manifest);
    }

    let scanner = Scanner::new(ScannerConfig::default());
    let ws_path = WorkspacePath::from_string(workspace_root.to_string_lossy().as_ref());
    let mut blob_store_ref = Some(&mut *store);
    Ok(scanner.scan_with_blob_store(&ws_path, &mut blob_store_ref)?)
}

pub(crate) fn load_checkpoint_repo_summaries(
    store: &SqliteStore,
    workspace_root: &Path,
    session_id: SessionId,
) -> Result<Vec<CheckpointRepoSummary>> {
    let mut summaries = Vec::new();
    for checkpoint in store.list_checkpoints(session_id)? {
        let Some(manifest_info) = store.get_manifest(checkpoint.manifest_id)? else {
            continue;
        };
        let manifest: Manifest = serde_json::from_str(&manifest_info.manifest_data)?;
        let repos = repo_patch_summaries(store, workspace_root, &manifest)?;
        if repos.is_empty() {
            continue;
        }
        summaries.push(CheckpointRepoSummary {
            checkpoint_id: checkpoint.id,
            repos,
        });
    }
    Ok(summaries)
}

pub(crate) fn materialize_checkpoint_repos(
    store: &SqliteStore,
    workspace_root: &Path,
    manifest: &Manifest,
    output_root: &Path,
) -> Result<Vec<MaterializedRepo>> {
    fs::create_dir_all(output_root)?;
    let artifacts = load_repo_patch_artifacts(store, workspace_root, manifest)?;
    let mut materialized = Vec::new();

    for artifact in artifacts {
        let destination = output_root.join(&artifact.slug);
        if destination.exists() {
            anyhow::bail!(
                "checkpoint replay destination already exists: {}",
                destination.display()
            );
        }

        let source_repo_root = artifact.metadata.repo_root.clone();
        let destination_display = destination.display().to_string();
        let base_commit = artifact.metadata.base_commit.clone();
        run_git_command(
            &[
                "clone",
                "--quiet",
                "--no-checkout",
                source_repo_root.as_str(),
                destination_display.as_str(),
            ],
            None,
        )?;
        run_git_command(
            &[
                "-C",
                destination_display.as_str(),
                "checkout",
                "--quiet",
                "--detach",
                base_commit.as_str(),
            ],
            None,
        )?;
        if !artifact.patch.is_empty() {
            run_git_command(
                &[
                    "-C",
                    destination_display.as_str(),
                    "apply",
                    "--binary",
                    "--whitespace=nowarn",
                    "-",
                ],
                Some(artifact.patch.as_bytes()),
            )?;
        }

        materialized.push(MaterializedRepo {
            repo_name: artifact.metadata.repo_name.clone(),
            slug: artifact.slug,
            source_repo_root,
            output_repo_root: destination_display,
            base_commit,
            apply_commit: artifact.metadata.apply_commit.clone(),
            patch_bytes: artifact.patch.len(),
        });
    }

    Ok(materialized)
}

fn build_repo_patch_manifest(
    store: &mut SqliteStore,
    workspace_root: &Path,
    created_at: &str,
) -> Result<Option<Manifest>> {
    let repo_targets = collect_repo_targets(workspace_root)?;
    if repo_targets.is_empty() {
        return Ok(None);
    }

    let mut manifest = Manifest::new(ManifestId::new());
    for target in repo_targets {
        let mut artifact = capture_repo_patch(&target, created_at)?;
        let metadata_path = format!("repos/{}/metadata.json", artifact.slug);
        let patch_path = format!("repos/{}/working-tree.patch", artifact.slug);
        let mut metadata_json = serde_json::to_vec_pretty(&artifact.metadata)?;
        maybe_omit_repo_patch_for_storage(store, &mut artifact, metadata_json.len() as u64)?;
        metadata_json = serde_json::to_vec_pretty(&artifact.metadata)?;
        let metadata_added = add_artifact_entry(
            store,
            &mut manifest,
            &metadata_path,
            &metadata_json,
            "application/json",
        )?;
        if metadata_added && artifact.metadata.capture_state.captured() {
            let _ = add_artifact_entry(
                store,
                &mut manifest,
                &patch_path,
                artifact.patch.as_bytes(),
                "text/x-diff",
            )?;
        }
    }
    manifest.finalize();
    Ok(Some(manifest))
}

fn collect_repo_targets(workspace_root: &Path) -> Result<Vec<RepoTarget>> {
    let Some(primary_root) = resolve_git_repo_root(workspace_root)? else {
        return Ok(Vec::new());
    };

    let mut seen = BTreeSet::new();
    let mut targets = vec![RepoTarget {
        role: PRIMARY_REPO_ROLE,
        root: primary_root.clone(),
    }];
    seen.insert(primary_root.clone());

    for root in companion_repo_roots(&primary_root)? {
        if seen.contains(&root) || !repo_has_changes(&root)? {
            continue;
        }
        seen.insert(root.clone());
        targets.push(RepoTarget {
            role: COMPANION_REPO_ROLE,
            root,
        });
    }

    Ok(targets)
}

fn companion_repo_roots(primary_root: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = cargo_path_dependency_roots(primary_root)?;
    let adapter_root = primary_root
        .parent()
        .map(|parent| parent.join("chatbot-adapter"));
    if let Some(adapter_root) = adapter_root {
        if adapter_root.exists() {
            roots.push(adapter_root);
        }
    }

    let mut resolved = BTreeSet::new();
    for root in roots {
        if let Some(repo_root) = resolve_git_repo_root(&root)? {
            if repo_root != primary_root {
                resolved.insert(repo_root);
            }
        }
    }

    Ok(resolved.into_iter().collect())
}

fn cargo_path_dependency_roots(primary_root: &Path) -> Result<Vec<PathBuf>> {
    let cargo_toml_path = primary_root.join("Cargo.toml");
    if !cargo_toml_path.is_file() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("failed to read {}", cargo_toml_path.display()))?;
    // Path dependency capture is best-effort. A manifest parse failure should
    // not disable history sync for the primary repo.
    let parsed = match toml::from_str::<toml::Table>(&raw) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(Vec::new()),
    };
    let mut roots = Vec::new();

    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        collect_path_dependencies_from_table(
            parsed.get(key).and_then(toml::Value::as_table),
            primary_root,
            &mut roots,
        );
    }

    if let Some(workspace_deps) = parsed
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("dependencies"))
        .and_then(toml::Value::as_table)
    {
        collect_path_dependencies_from_table(Some(workspace_deps), primary_root, &mut roots);
    }

    Ok(roots)
}

fn collect_path_dependencies_from_table(
    table: Option<&toml::value::Table>,
    primary_root: &Path,
    roots: &mut Vec<PathBuf>,
) {
    let Some(table) = table else {
        return;
    };

    for value in table.values() {
        let Some(dep_table) = value.as_table() else {
            continue;
        };
        let Some(path_value) = dep_table.get("path").and_then(|value| value.as_str()) else {
            continue;
        };
        roots.push(primary_root.join(path_value));
    }
}

fn resolve_git_repo_root(path: &Path) -> Result<Option<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--show-toplevel"])
        .output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }

    let root = String::from_utf8(output.stdout)?.trim().to_string();
    if root.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(root)))
}

fn repo_has_changes(repo_root: &Path) -> Result<bool> {
    let output = git_output(
        repo_root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
        &[0],
    )?;
    Ok(!output.trim().is_empty())
}

fn capture_repo_patch(target: &RepoTarget, created_at: &str) -> Result<RepoPatchArtifact> {
    let apply_commit = git_output(target.root.as_path(), &["rev-parse", "HEAD"], &[0])?;
    let tracked_patch = git_output(
        target.root.as_path(),
        &["diff", "--binary", "--find-renames", "HEAD", "--"],
        &[0],
    )?;
    let untracked_files = git_output(
        target.root.as_path(),
        &["ls-files", "--others", "--exclude-standard"],
        &[0],
    )?
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| line.to_string())
    .collect::<Vec<_>>();

    let max_patch_bytes = max_repo_patch_bytes();
    let repo_name = target
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo")
        .to_string();
    if tracked_patch.len() as u64 > max_patch_bytes {
        return Ok(omitted_repo_patch_artifact(
            &repo_name,
            target,
            created_at,
            apply_commit.trim(),
            tracked_patch.len(),
            tracked_patch.len(),
            untracked_files.len(),
            RepoPatchCaptureState::OmittedOversize,
            format!(
                "tracked diff exceeded the {} byte repo-patch limit",
                max_patch_bytes
            ),
        ));
    }

    let mut patch = tracked_patch.clone();
    for relative_path in &untracked_files {
        let relative_file = target.root.join(relative_path);
        let file_size = fs::metadata(&relative_file)
            .map(|metadata| metadata.len())
            .ok();
        if file_size.is_some_and(|size| size > max_patch_bytes) {
            return Ok(omitted_repo_patch_artifact(
                &repo_name,
                target,
                created_at,
                apply_commit.trim(),
                tracked_patch.len(),
                patch
                    .len()
                    .saturating_add(file_size.unwrap_or_default() as usize),
                untracked_files.len(),
                RepoPatchCaptureState::OmittedOversize,
                format!(
                    "untracked file `{}` exceeded the {} byte repo-patch limit",
                    relative_path, max_patch_bytes
                ),
            ));
        }
        let file_patch = git_output(
            target.root.as_path(),
            &[
                "diff",
                "--binary",
                "--no-index",
                "--",
                "/dev/null",
                relative_path,
            ],
            &[1],
        )?;
        if patch.len().saturating_add(file_patch.len()) as u64 > max_patch_bytes {
            return Ok(omitted_repo_patch_artifact(
                &repo_name,
                target,
                created_at,
                apply_commit.trim(),
                tracked_patch.len(),
                patch.len().saturating_add(file_patch.len()),
                untracked_files.len(),
                RepoPatchCaptureState::OmittedOversize,
                format!(
                    "dirty repo patch exceeded the {} byte repo-patch limit",
                    max_patch_bytes
                ),
            ));
        }
        patch.push_str(&file_patch);
        if !patch.ends_with('\n') {
            patch.push('\n');
        }
    }

    Ok(RepoPatchArtifact {
        slug: slugify_repo_name(&repo_name),
        metadata: RepoPatchMetadata {
            schema_version: PATCH_SCHEMA_VERSION,
            repo_name,
            repo_root: target.root.display().to_string(),
            role: target.role.to_string(),
            base_commit: apply_commit.trim().to_string(),
            apply_commit: apply_commit.trim().to_string(),
            patch_format: PATCH_FORMAT.to_string(),
            tracked_patch_bytes: tracked_patch.len(),
            untracked_file_count: untracked_files.len(),
            patch_is_empty: patch.is_empty(),
            total_patch_bytes: patch.len(),
            capture_state: RepoPatchCaptureState::Captured,
            omitted_reason: None,
            captured_at: created_at.to_string(),
        },
        patch,
    })
}

fn add_artifact_entry(
    store: &mut SqliteStore,
    manifest: &mut Manifest,
    path: &str,
    contents: &[u8],
    mime_type: &str,
) -> Result<bool> {
    let blob_hash = ContentHash::new(contents);
    match store.write_blob(blob_hash.as_bytes(), contents) {
        Ok(()) => {}
        Err(err) => {
            let message = err.to_string();
            if message.contains("blob quota exceeded") {
                return Ok(false);
            }
            return Err(anyhow::anyhow!(message));
        }
    }
    manifest.add_entry(FileEntry {
        path: path.to_string(),
        blob_hash,
        size: contents.len() as u64,
        mode: 0o644,
        mime_type: Some(mime_type.to_string()),
        executable: false,
        symlink_target: None,
        modified: None,
        ownership: None,
    });
    Ok(true)
}

fn omitted_repo_patch_artifact(
    repo_name: &str,
    target: &RepoTarget,
    created_at: &str,
    apply_commit: &str,
    tracked_patch_bytes: usize,
    total_patch_bytes: usize,
    untracked_file_count: usize,
    capture_state: RepoPatchCaptureState,
    omitted_reason: String,
) -> RepoPatchArtifact {
    RepoPatchArtifact {
        slug: slugify_repo_name(repo_name),
        metadata: RepoPatchMetadata {
            schema_version: PATCH_SCHEMA_VERSION,
            repo_name: repo_name.to_string(),
            repo_root: target.root.display().to_string(),
            role: target.role.to_string(),
            base_commit: apply_commit.to_string(),
            apply_commit: apply_commit.to_string(),
            patch_format: PATCH_FORMAT.to_string(),
            tracked_patch_bytes,
            untracked_file_count,
            patch_is_empty: false,
            total_patch_bytes,
            capture_state,
            omitted_reason: Some(omitted_reason),
            captured_at: created_at.to_string(),
        },
        patch: String::new(),
    }
}

fn maybe_omit_repo_patch_for_storage(
    store: &SqliteStore,
    artifact: &mut RepoPatchArtifact,
    metadata_bytes: u64,
) -> Result<()> {
    if !artifact.metadata.capture_state.captured() || artifact.patch.is_empty() {
        return Ok(());
    }

    let quota = store.quota_report()?;
    let remaining_bytes = quota
        .max_total_blob_bytes
        .saturating_sub(quota.stored_bytes);
    let required_bytes = metadata_bytes.saturating_add(artifact.patch.len() as u64);
    if required_bytes <= remaining_bytes {
        return Ok(());
    }

    artifact.metadata.capture_state = RepoPatchCaptureState::OmittedQuota;
    artifact.metadata.omitted_reason = Some(format!(
        "repo patch exceeded the remaining history blob budget ({} B available)",
        remaining_bytes
    ));
    artifact.patch.clear();
    Ok(())
}

fn max_repo_patch_bytes() -> u64 {
    std::env::var("RUST_CHATBOT_HISTORY_MAX_REPO_PATCH_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_REPO_PATCH_BYTES)
}

fn repo_patch_summaries(
    store: &SqliteStore,
    workspace_root: &Path,
    manifest: &Manifest,
) -> Result<Vec<RepoPatchSummary>> {
    let mut summaries = Vec::new();
    for metadata_path in repo_patch_metadata_paths(manifest) {
        // Repo summary projection is best-effort. A single corrupted checkpoint
        // artifact should not blank the entire History panel.
        let Ok(artifact) =
            load_repo_patch_artifact(store, workspace_root, manifest, &metadata_path)
        else {
            continue;
        };
        summaries.push(RepoPatchSummary {
            slug: artifact.slug,
            repo_name: artifact.metadata.repo_name,
            role: artifact.metadata.role,
            repo_root: artifact.metadata.repo_root,
            base_commit: artifact.metadata.base_commit,
            apply_commit: artifact.metadata.apply_commit,
            patch_bytes: artifact.patch.len(),
            tracked_patch_bytes: artifact.metadata.tracked_patch_bytes,
            untracked_file_count: artifact.metadata.untracked_file_count,
            patch_is_empty: artifact.metadata.patch_is_empty,
            total_patch_bytes: artifact.metadata.total_patch_bytes,
            capture_state: artifact.metadata.capture_state,
            omitted_reason: artifact.metadata.omitted_reason,
            captured_at: artifact.metadata.captured_at,
        });
    }
    Ok(summaries)
}

fn load_repo_patch_artifacts(
    store: &SqliteStore,
    workspace_root: &Path,
    manifest: &Manifest,
) -> Result<Vec<RepoPatchArtifact>> {
    let mut artifacts = Vec::new();
    for metadata_path in repo_patch_metadata_paths(manifest) {
        artifacts.push(load_repo_patch_artifact(
            store,
            workspace_root,
            manifest,
            &metadata_path,
        )?);
    }

    Ok(artifacts)
}

fn repo_patch_metadata_paths(manifest: &Manifest) -> Vec<String> {
    let mut metadata_paths = manifest
        .entries
        .keys()
        .filter(|path| path.starts_with("repos/") && path.ends_with("/metadata.json"))
        .cloned()
        .collect::<Vec<_>>();
    metadata_paths.sort();
    metadata_paths
}

fn load_repo_patch_artifact(
    store: &SqliteStore,
    workspace_root: &Path,
    manifest: &Manifest,
    metadata_path: &str,
) -> Result<RepoPatchArtifact> {
    let slug = metadata_path
        .strip_prefix("repos/")
        .and_then(|path| path.strip_suffix("/metadata.json"))
        .ok_or_else(|| anyhow::anyhow!("invalid repo patch metadata path {}", metadata_path))?;

    let metadata_entry = manifest
        .get_entry(metadata_path)
        .ok_or_else(|| anyhow::anyhow!("missing manifest entry for {}", metadata_path))?;
    let metadata_bytes = read_artifact_entry(store, workspace_root, metadata_entry.blob_hash)?
        .ok_or_else(|| anyhow::anyhow!("missing blob for {}", metadata_path))?;
    let metadata = serde_json::from_slice::<RepoPatchMetadata>(&metadata_bytes)?;

    let patch_path = format!("repos/{slug}/working-tree.patch");
    let patch = match manifest.get_entry(patch_path.as_str()) {
        Some(patch_entry) => read_artifact_entry(store, workspace_root, patch_entry.blob_hash)?
            .map(String::from_utf8)
            .transpose()?
            .unwrap_or_default(),
        None => String::new(),
    };

    Ok(RepoPatchArtifact {
        slug: slug.to_string(),
        metadata,
        patch,
    })
}

fn read_artifact_entry(
    store: &SqliteStore,
    workspace_root: &Path,
    blob_hash: ContentHash,
) -> Result<Option<Vec<u8>>> {
    if let Some(bytes) = read_or_restore_history_object(workspace_root, &blob_hash)? {
        return Ok(Some(bytes));
    }
    Ok(store.read_blob(blob_hash.as_bytes())?)
}

fn git_output(repo_root: &Path, args: &[&str], success_codes: &[i32]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo_root.display()))?;
    let code = output.status.code().unwrap_or_default();
    if !success_codes.contains(&code) {
        anyhow::bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn run_git_command(args: &[&str], stdin_bytes: Option<&[u8]>) -> Result<()> {
    let mut command = Command::new("git");
    command.args(args);
    if stdin_bytes.is_some() {
        command.stdin(std::process::Stdio::piped());
    }
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if let Some(stdin_bytes) = stdin_bytes {
        let Some(mut stdin) = child.stdin.take() else {
            anyhow::bail!("failed to open git stdin for {}", args.join(" "));
        };
        stdin.write_all(stdin_bytes)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn slugify_repo_name(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
#[path = "history_repo_capture_tests.rs"]
mod history_repo_capture_tests;
