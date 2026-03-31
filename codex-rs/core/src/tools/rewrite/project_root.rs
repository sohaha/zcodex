use codex_git_utils::get_git_repo_root;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn resolve_tldr_project_root(cwd: &Path, target: Option<&Path>) -> PathBuf {
    target
        .and_then(get_git_repo_root)
        .or_else(|| get_git_repo_root(cwd))
        .unwrap_or_else(|| cwd.to_path_buf())
}
