use crate::config::ZMEMORY_DB_FILENAME;
use crate::config::ZMEMORY_DIR;
use crate::config::ZMEMORY_PROJECTS_DIR;
use crate::config::project_key_for_workspace;
use anyhow::Context;
use anyhow::Result;
use codex_git_utils::resolve_root_git_project_for_trust;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZmemoryPathResolution {
    pub db_path: PathBuf,
    pub workspace_key: Option<String>,
    pub source: ZmemoryPathSource,
    pub canonical_base: Option<PathBuf>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ZmemoryPathSource {
    Explicit,
    ProjectScoped,
}

pub fn resolve_zmemory_path(
    codex_home: &Path,
    cwd: &Path,
    explicit_path: Option<&Path>,
) -> Result<ZmemoryPathResolution> {
    if let Some(raw_path) = explicit_path {
        if raw_path.as_os_str().is_empty() {
            anyhow::bail!("zmemory.path cannot be empty");
        }
        return resolve_explicit_zmemory_path(cwd, raw_path);
    }

    let canonical_codex_home = canonicalize_existing_path(codex_home)
        .with_context(|| format!("canonicalize {}", codex_home.display()))?;
    let canonical_workspace_base = resolve_workspace_base_path(cwd)?;
    let workspace_key = project_key_for_workspace(&canonical_workspace_base);
    let db_path = canonical_codex_home
        .join(ZMEMORY_DIR)
        .join(ZMEMORY_PROJECTS_DIR)
        .join(&workspace_key)
        .join(ZMEMORY_DB_FILENAME);
    let anchor_label = workspace_anchor_label(cwd);
    let reason = format!(
        "defaulted to project scope {} from {anchor_label} {}",
        db_path.display(),
        canonical_workspace_base.display()
    );

    Ok(ZmemoryPathResolution {
        db_path,
        workspace_key: Some(workspace_key),
        source: ZmemoryPathSource::ProjectScoped,
        canonical_base: None,
        reason,
    })
}

fn resolve_explicit_zmemory_path(cwd: &Path, raw_path: &Path) -> Result<ZmemoryPathResolution> {
    let base = resolve_workspace_base(cwd);
    let (db_path, canonical_base, source_reason) = if raw_path.is_absolute() {
        (
            canonicalize_file_path(raw_path)?,
            None,
            "absolute path".to_string(),
        )
    } else {
        let canonical_base_path = canonicalize_existing_path(&base)
            .with_context(|| format!("canonicalize {}", base.display()))?;
        let db_path = canonicalize_file_path(&canonical_base_path.join(raw_path))?;
        let anchor_label = workspace_anchor_label(cwd);
        let source_reason = format!(
            "relative path resolved from {anchor_label} {}",
            canonical_base_path.display()
        );
        (db_path, Some(canonical_base_path), source_reason)
    };
    let raw_display = raw_path.display();

    Ok(ZmemoryPathResolution {
        db_path,
        workspace_key: None,
        source: ZmemoryPathSource::Explicit,
        canonical_base,
        reason: format!("explicit zmemory.path `{raw_display}` via {source_reason}"),
    })
}

pub fn resolve_workspace_base(cwd: &Path) -> PathBuf {
    if let Some(repo_root) = resolve_root_git_project_for_trust(cwd) {
        repo_root
    } else {
        cwd.to_path_buf()
    }
}

pub fn resolve_workspace_base_path(cwd: &Path) -> Result<PathBuf> {
    let base = resolve_workspace_base(cwd);
    canonicalize_existing_path(&base).with_context(|| format!("canonicalize {}", base.display()))
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path)
        .with_context(|| format!("path does not exist or is inaccessible: {}", path.display()))
}

fn canonicalize_file_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return canonicalize_existing_path(path);
    }

    let parent = path.parent().context("db path has no parent directory")?;
    let canonical_parent = canonicalize_existing_path(parent)?;
    let file_name = path.file_name().context("db path has no file name")?;
    Ok(canonical_parent.join(file_name))
}

fn workspace_anchor_label(cwd: &Path) -> &'static str {
    if resolve_root_git_project_for_trust(cwd).is_some() {
        "repo root"
    } else {
        "cwd"
    }
}

#[cfg(test)]
mod tests {
    use super::ZmemoryPathSource;
    use super::project_key_for_workspace;
    use super::resolve_workspace_base_path;
    use super::resolve_zmemory_path;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn defaults_to_project_scoped_path() {
        let temp = TempDir::new().expect("tempdir");
        let repo = temp.path().join("repo");
        let nested = repo.join("nested");
        let codex_home = temp.path().join("codex-home");
        fs::create_dir_all(repo.join(".git")).expect("create git dir");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::create_dir_all(&codex_home).expect("create codex home");

        let resolution = resolve_zmemory_path(&codex_home, &nested, None).expect("resolve path");

        let workspace_key =
            project_key_for_workspace(&repo.canonicalize().expect("canonical repo"));
        assert_eq!(resolution.source, ZmemoryPathSource::ProjectScoped);
        assert_eq!(
            resolution.db_path,
            codex_home
                .join("zmemory")
                .join("projects")
                .join(&workspace_key)
                .join("zmemory.db")
        );
        assert_eq!(
            resolution.workspace_key.as_deref(),
            Some(workspace_key.as_str())
        );
        assert!(resolution.reason.contains("defaulted to project scope"));
    }

    #[test]
    fn worktree_workspace_base_resolves_to_main_repo_root() {
        let temp = TempDir::new().expect("tempdir");
        let main_repo = temp.path().join("main");
        let worktree = temp.path().join("wt");
        let worktrees_dir = main_repo.join(".git").join("worktrees").join("feature");
        fs::create_dir_all(&worktrees_dir).expect("create worktrees dir");
        fs::create_dir_all(&worktree).expect("create worktree");
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktrees_dir.display()),
        )
        .expect("write .git file");

        let workspace_base = resolve_workspace_base_path(&worktree).expect("workspace base");
        let canonical_repo = main_repo.canonicalize().expect("canonical repo");

        assert_eq!(workspace_base, canonical_repo);
    }

    #[test]
    fn workspace_base_defaults_to_cwd_outside_git() {
        let temp = TempDir::new().expect("tempdir");
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd).expect("create cwd");

        let workspace_base = resolve_workspace_base_path(&cwd).expect("workspace base");

        assert_eq!(workspace_base, cwd.canonicalize().expect("canonical cwd"));
    }

    #[test]
    fn relative_explicit_path_uses_repo_root_anchor() {
        let temp = TempDir::new().expect("tempdir");
        let repo = temp.path().join("repo");
        let nested = repo.join("nested");
        fs::create_dir_all(repo.join(".git")).expect("create git dir");
        fs::create_dir_all(repo.join("agents")).expect("create agents dir");
        fs::create_dir_all(&nested).expect("create nested dir");

        let resolution =
            resolve_zmemory_path(temp.path(), &nested, Some(Path::new("./agents/memory.db")))
                .expect("resolve path");

        assert_eq!(resolution.source, ZmemoryPathSource::Explicit);
        assert_eq!(resolution.db_path, repo.join("agents").join("memory.db"));
        assert_eq!(
            resolution.canonical_base.as_deref(),
            Some(repo.canonicalize().expect("canonical repo").as_path())
        );
        assert!(
            resolution
                .reason
                .contains("relative path resolved from repo root")
        );
    }

    #[test]
    fn relative_explicit_path_uses_cwd_anchor_outside_git() {
        let temp = TempDir::new().expect("tempdir");
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd).expect("create cwd");

        let resolution = resolve_zmemory_path(temp.path(), &cwd, Some(Path::new("./memory.db")))
            .expect("resolve path");

        assert_eq!(resolution.source, ZmemoryPathSource::Explicit);
        assert_eq!(resolution.db_path, cwd.join("memory.db"));
        assert_eq!(
            resolution.canonical_base.as_deref(),
            Some(cwd.canonicalize().expect("canonical cwd").as_path())
        );
        assert!(
            resolution
                .reason
                .contains("relative path resolved from cwd")
        );
    }

    #[test]
    fn missing_anchor_for_explicit_relative_path_errors() {
        let temp = TempDir::new().expect("tempdir");
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd).expect("create cwd");

        let error = resolve_zmemory_path(temp.path(), &cwd, Some(Path::new("./missing/memory.db")))
            .expect_err("missing parent should fail");

        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn explicit_global_db_path_is_allowed() {
        let temp = TempDir::new().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let db_path = codex_home.join("zmemory").join("zmemory.db");
        fs::create_dir_all(db_path.parent().expect("db parent")).expect("create db parent");

        let resolution = resolve_zmemory_path(&codex_home, Path::new("/"), Some(db_path.as_path()))
            .expect("resolve path");

        assert_eq!(resolution.source, ZmemoryPathSource::Explicit);
        assert_eq!(resolution.db_path, db_path);
    }

    #[test]
    fn worktree_and_main_repo_share_same_default_project_key() {
        let temp = TempDir::new().expect("tempdir");
        let main_repo = temp.path().join("main");
        let worktree = temp.path().join("wt");
        let codex_home = temp.path().join("codex-home");
        let worktrees_dir = main_repo.join(".git").join("worktrees").join("feature");
        fs::create_dir_all(&worktrees_dir).expect("create worktrees dir");
        fs::create_dir_all(main_repo.join("src")).expect("create main repo");
        fs::create_dir_all(&worktree).expect("create worktree");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktrees_dir.display()),
        )
        .expect("write .git file");

        let main_resolution =
            resolve_zmemory_path(&codex_home, main_repo.join("src").as_path(), None)
                .expect("main repo resolution");
        let worktree_resolution =
            resolve_zmemory_path(&codex_home, &worktree, None).expect("worktree resolution");

        assert_eq!(
            main_resolution.workspace_key,
            worktree_resolution.workspace_key
        );
        assert_eq!(main_resolution.db_path, worktree_resolution.db_path);
    }
}
