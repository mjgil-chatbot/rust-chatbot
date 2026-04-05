const BACKUP_SCRIPT_RELATIVE_PATH: &str = "scripts/backup-llm-data.sh";
fn current_executable_path() -> Result<PathBuf> {
    let path = std::env::current_exe().context("failed to resolve current executable")?;
    Ok(path.canonicalize().ok().unwrap_or(path))
}
fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}
fn backup_script_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = current_executable_path() {
        let current_dir = current_exe.parent().unwrap_or(Path::new("."));
        if let Some(target_dir) = current_dir.parent() {
            if let Some(repo_root) = target_dir.parent() {
                push_unique_path(&mut candidates, repo_root.join(BACKUP_SCRIPT_RELATIVE_PATH));
            }
        }
    }

    push_unique_path(
        &mut candidates,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BACKUP_SCRIPT_RELATIVE_PATH),
    );

    if let Ok(cwd) = std::env::current_dir() {
        push_unique_path(&mut candidates, cwd.join(BACKUP_SCRIPT_RELATIVE_PATH));
    }

    candidates
}
fn backup_script_path() -> Result<PathBuf> {
    for candidate in backup_script_candidates() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "could not locate {} from the current executable or current directory",
        BACKUP_SCRIPT_RELATIVE_PATH
    );
}
fn backup_repo_root() -> Result<PathBuf> {
    let script_path = backup_script_path()?;
    let scripts_dir = script_path
        .parent()
        .context("backup script should live in the scripts directory")?;
    let repo_root = scripts_dir
        .parent()
        .context("scripts directory should have a repo root parent")?;
    Ok(repo_root.to_path_buf())
}
fn systemd_unit_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/systemd/user")
}
