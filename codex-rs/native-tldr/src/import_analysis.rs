use crate::TldrConfig;
use crate::api::ImporterMatch;
use crate::api::ImportersRequest;
use crate::api::ImportersResponse;
use crate::api::ImportsRequest;
use crate::api::ImportsResponse;
use crate::semantic::SemanticIndexer;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn collect_imports(
    project_root: &Path,
    config: &TldrConfig,
    request: ImportsRequest,
) -> Result<ImportsResponse> {
    let index = SemanticIndexer::new(config.semantic.clone())
        .load_or_build_index(project_root, request.language)?;
    let normalized_path = normalize_request_path(project_root, &request.path)?;
    let imports = index
        .units
        .iter()
        .filter(|unit| unit.path == normalized_path)
        .flat_map(|unit| unit.imports.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(ImportsResponse {
        language: request.language,
        path: normalized_path.display().to_string(),
        indexed_files: index.indexed_files,
        imports,
    })
}

pub(crate) fn collect_importers(
    project_root: &Path,
    config: &TldrConfig,
    request: ImportersRequest,
) -> Result<ImportersResponse> {
    let index = SemanticIndexer::new(config.semantic.clone())
        .load_or_build_index(project_root, request.language)?;
    let matches = index
        .units
        .iter()
        .flat_map(|unit| {
            unit.imports
                .iter()
                .filter(|&import| import.contains(&request.module))
                .map(|import| ImporterMatch {
                    path: unit.path.display().to_string(),
                    line: unit.line,
                    symbol: unit.symbol.clone(),
                    import: import.clone(),
                })
        })
        .collect();
    Ok(ImportersResponse {
        language: request.language,
        module: request.module,
        indexed_files: index.indexed_files,
        matches,
    })
}

fn normalize_request_path(project_root: &Path, path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    let absolute = if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    };
    let normalized = absolute.canonicalize().unwrap_or(absolute);
    Ok(normalized
        .strip_prefix(project_root)
        .map(Path::to_path_buf)
        .unwrap_or(normalized))
}

#[cfg(test)]
mod tests {
    use super::collect_importers;
    use super::collect_imports;
    use crate::TldrConfig;
    use crate::api::ImportersRequest;
    use crate::api::ImportsRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn collect_imports_returns_unique_file_imports() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "use crate::auth::token;\n\nfn login() {}\n",
        )
        .expect("fixture should write");

        let response = collect_imports(
            tempdir.path(),
            &TldrConfig::for_project(tempdir.path().to_path_buf()),
            ImportsRequest {
                language: SupportedLanguage::Rust,
                path: "src/lib.rs".to_string(),
            },
        )
        .expect("imports should succeed");

        assert_eq!(response.path, "src/lib.rs");
        assert_eq!(
            response.imports,
            vec!["use crate::auth::token;".to_string()]
        );
    }

    #[test]
    fn collect_importers_returns_matching_units() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "use crate::auth::token;\n\nfn login() {}\n",
        )
        .expect("fixture should write");

        let response = collect_importers(
            tempdir.path(),
            &TldrConfig::for_project(tempdir.path().to_path_buf()),
            ImportersRequest {
                language: SupportedLanguage::Rust,
                module: "auth::token".to_string(),
            },
        )
        .expect("importers should succeed");

        assert_eq!(response.module, "auth::token");
        assert_eq!(response.matches.len(), 1);
        assert_eq!(response.matches[0].path, "src/lib.rs");
        assert_eq!(response.matches[0].symbol.as_deref(), Some("login"));
        assert_eq!(response.matches[0].import, "use crate::auth::token;");
    }
}
