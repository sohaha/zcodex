use crate::api::SearchMatch;
use crate::api::SearchMatchMode;
use crate::api::SearchRequest;
use crate::api::SearchResponse;
use crate::lang_support::SupportedLanguage;
use anyhow::Result;
use ignore::WalkBuilder;
use regex::Regex;
use std::path::Path;

pub(crate) fn search_project(
    project_root: &Path,
    request: SearchRequest,
) -> Result<SearchResponse> {
    let pattern = match request.match_mode {
        SearchMatchMode::Literal => Regex::new(&regex::escape(&request.pattern))
            .expect("escaped literal search pattern should always compile"),
        SearchMatchMode::Regex => Regex::new(&request.pattern).map_err(|error| {
            anyhow::anyhow!("invalid regex pattern `{}`: {error}", request.pattern)
        })?,
    };
    let mut matches = Vec::new();
    let mut indexed_files = 0usize;
    let limit = request.max_results.max(1);
    let language = request.language;

    let mut walker = WalkBuilder::new(project_root);
    walker
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true);

    for entry in walker.build() {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path.is_file() || !matches_language(path, language) {
            continue;
        }
        indexed_files += 1;

        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        let relative_path = path
            .strip_prefix(project_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf());
        for (index, line) in contents.lines().enumerate() {
            if pattern.is_match(line) {
                matches.push(SearchMatch {
                    path: relative_path.display().to_string(),
                    line: index + 1,
                    content: line.trim().to_string(),
                });
                if matches.len() >= limit {
                    return Ok(SearchResponse {
                        pattern: request.pattern,
                        match_mode: request.match_mode,
                        indexed_files,
                        truncated: true,
                        matches,
                    });
                }
            }
        }
    }

    Ok(SearchResponse {
        pattern: request.pattern,
        match_mode: request.match_mode,
        indexed_files,
        truncated: false,
        matches,
    })
}

fn matches_language(path: &Path, language: Option<SupportedLanguage>) -> bool {
    match language {
        Some(language) => SupportedLanguage::from_path(path) == Some(language),
        None => SupportedLanguage::from_path(path).is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::search_project;
    use crate::api::SearchMatchMode;
    use crate::api::SearchRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn search_project_returns_regex_matches_when_requested() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn login() {}\nfn logout() {}\n",
        )
        .expect("fixture should write");

        let response = search_project(
            tempdir.path(),
            SearchRequest {
                pattern: "log(in|out)".to_string(),
                match_mode: SearchMatchMode::Regex,
                language: Some(SupportedLanguage::Rust),
                max_results: 10,
            },
        )
        .expect("search should succeed");

        assert_eq!(response.match_mode, SearchMatchMode::Regex);
        assert_eq!(response.indexed_files, 1);
        assert_eq!(response.matches.len(), 2);
        assert_eq!(response.matches[0].line, 1);
    }

    #[test]
    fn search_project_defaults_to_literal_matching_for_regex_metacharacters() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "resolveProjectAvatar(\n[workspaces/get] start\n",
        )
        .expect("fixture should write");

        let paren_response = search_project(
            tempdir.path(),
            SearchRequest {
                pattern: "resolveProjectAvatar(".to_string(),
                match_mode: SearchMatchMode::Literal,
                language: Some(SupportedLanguage::Rust),
                max_results: 10,
            },
        )
        .expect("literal search should succeed");
        let bracket_response = search_project(
            tempdir.path(),
            SearchRequest {
                pattern: "[workspaces/get] start".to_string(),
                match_mode: SearchMatchMode::Literal,
                language: Some(SupportedLanguage::Rust),
                max_results: 10,
            },
        )
        .expect("literal search should succeed");

        assert_eq!(paren_response.match_mode, SearchMatchMode::Literal);
        assert_eq!(paren_response.matches.len(), 1);
        assert_eq!(paren_response.matches[0].content, "resolveProjectAvatar(");
        assert_eq!(bracket_response.matches.len(), 1);
        assert_eq!(
            bracket_response.matches[0].content,
            "[workspaces/get] start"
        );
    }

    #[test]
    fn search_project_reports_invalid_regex_patterns() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(tempdir.path().join("src/lib.rs"), "resolveProjectAvatar(\n")
            .expect("fixture should write");

        let error = search_project(
            tempdir.path(),
            SearchRequest {
                pattern: "resolveProjectAvatar(".to_string(),
                match_mode: SearchMatchMode::Regex,
                language: Some(SupportedLanguage::Rust),
                max_results: 10,
            },
        )
        .expect_err("invalid regex should fail");

        let message = error.to_string();
        assert!(message.contains("invalid regex pattern `resolveProjectAvatar(`"));
        assert!(message.contains("unclosed group"));
    }
}
