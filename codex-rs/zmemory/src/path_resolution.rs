use crate::config::ZMEMORY_DB_FILENAME;
use crate::config::ZMEMORY_DIR;
use anyhow::Context;
use anyhow::Result;
use codex_git_utils::resolve_root_git_project_for_trust;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::path::PathBuf;

const WORKSPACE_PREFIX: &str = "workspace-";
const WORKSPACE_KEY_LEN: usize = 12;

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
    RepoRoot,
    Cwd,
}

pub fn resolve_zmemory_path(
    codex_home: &Path,
    cwd: &Path,
    explicit_path: Option<&Path>,
) -> Result<ZmemoryPathResolution> {
    if let Some(raw_path) = explicit_path {
        if raw_path.as_os_str().is_empty() {
            anyhow::bail!("zmemory_path cannot be empty");
        }
        return resolve_explicit_zmemory_path(cwd, raw_path);
    }

    let (source, base) = resolve_workspace_base(cwd);
    let canonical_base = canonicalize_existing_path(&base)
        .with_context(|| format!("canonicalize {}", base.display()))?;
    let workspace_key = format!(
        "{WORKSPACE_PREFIX}{}",
        workspace_key_for_path(&canonical_base)
    );
    let db_path = codex_home
        .join(ZMEMORY_DIR)
        .join(&workspace_key)
        .join(ZMEMORY_DB_FILENAME);
    let reason = match source {
        ZmemoryPathSource::RepoRoot => {
            format!("defaulted to repo root {}", canonical_base.display())
        }
        ZmemoryPathSource::Cwd => format!("defaulted to cwd {}", canonical_base.display()),
        ZmemoryPathSource::Explicit => unreachable!("explicit paths return early"),
    };

    Ok(ZmemoryPathResolution {
        db_path,
        workspace_key: Some(workspace_key),
        source,
        canonical_base: Some(canonical_base),
        reason,
    })
}

fn resolve_explicit_zmemory_path(cwd: &Path, raw_path: &Path) -> Result<ZmemoryPathResolution> {
    let (base_source, base) = resolve_workspace_base(cwd);
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
        let anchor_label = match base_source {
            ZmemoryPathSource::RepoRoot => "repo root",
            ZmemoryPathSource::Cwd => "cwd",
            ZmemoryPathSource::Explicit => unreachable!("explicit path cannot anchor itself"),
        };
        let source_reason = format!(
            "relative path resolved from {anchor_label} {}",
            canonical_base_path.display()
        );
        (db_path, Some(canonical_base_path.clone()), source_reason)
    };
    let raw_display = raw_path.display();

    Ok(ZmemoryPathResolution {
        db_path,
        workspace_key: None,
        source: ZmemoryPathSource::Explicit,
        canonical_base,
        reason: format!("explicit zmemory_path `{raw_display}` via {source_reason}"),
    })
}

fn resolve_workspace_base(cwd: &Path) -> (ZmemoryPathSource, PathBuf) {
    if let Some(repo_root) = resolve_root_git_project_for_trust(cwd) {
        (ZmemoryPathSource::RepoRoot, repo_root)
    } else {
        (ZmemoryPathSource::Cwd, cwd.to_path_buf())
    }
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

fn workspace_key_for_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(WORKSPACE_KEY_LEN);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        if hex.len() >= WORKSPACE_KEY_LEN {
            hex.truncate(WORKSPACE_KEY_LEN);
            break;
        }
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::WORKSPACE_PREFIX;
    use super::ZmemoryPathSource;
    use super::resolve_zmemory_path;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn defaults_to_repo_root_workspace_key() {
        let temp = TempDir::new().expect("tempdir");
        let repo = temp.path().join("repo");
        let nested = repo.join("nested");
        fs::create_dir_all(repo.join(".git")).expect("create git dir");
        fs::create_dir_all(&nested).expect("create nested dir");

        let resolution = resolve_zmemory_path(temp.path(), &nested, None).expect("resolve path");
        let canonical_repo = repo.canonicalize().expect("canonical repo");

        assert_eq!(resolution.source, ZmemoryPathSource::RepoRoot);
        assert_eq!(
            resolution.canonical_base.as_deref(),
            Some(canonical_repo.as_path())
        );
        assert_eq!(
            resolution
                .workspace_key
                .as_deref()
                .map(|key| key.starts_with(WORKSPACE_PREFIX)),
            Some(true)
        );
    }

    #[test]
    fn worktree_defaults_to_main_repo_root() {
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

        let resolution = resolve_zmemory_path(temp.path(), &worktree, None).expect("resolve path");
        let canonical_repo = main_repo.canonicalize().expect("canonical repo");

        assert_eq!(resolution.source, ZmemoryPathSource::RepoRoot);
        assert_eq!(
            resolution.canonical_base.as_deref(),
            Some(canonical_repo.as_path())
        );
    }

    #[test]
    fn defaults_to_cwd_outside_git() {
        let temp = TempDir::new().expect("tempdir");
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd).expect("create cwd");

        let resolution = resolve_zmemory_path(temp.path(), &cwd, None).expect("resolve path");
        let canonical_cwd = cwd.canonicalize().expect("canonical cwd");

        assert_eq!(resolution.source, ZmemoryPathSource::Cwd);
        assert_eq!(
            resolution.canonical_base.as_deref(),
            Some(canonical_cwd.as_path())
        );
        assert!(resolution.reason.contains("cwd"));
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
}
