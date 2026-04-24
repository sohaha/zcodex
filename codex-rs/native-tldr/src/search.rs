use crate::api::SearchMatch;
use crate::api::SearchMatchMode;
use crate::api::SearchRequest;
use crate::api::SearchResponse;
use crate::lang_support::SupportedLanguage;
use anyhow::Context;
use anyhow::Result;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::Value;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

const ALL_LANGUAGE_GLOBS: &[&str] = &[
    "*.c", "*.h", "*.cpp", "*.cc", "*.cxx", "*.hpp", "*.hh", "*.hxx", "*.cs", "*.go", "*.java",
    "*.js", "*.jsx", "*.mjs", "*.cjs", "*.kt", "*.kts", "*.lua", "*.luau", "*.php", "*.py", "*.rb",
    "*.rs", "*.swift", "*.ts", "*.tsx", "*.zig",
];

pub(crate) fn search_project(
    project_root: &Path,
    request: SearchRequest,
) -> Result<SearchResponse> {
    validate_regex_pattern(&request)?;
    match search_project_with_ripgrep(project_root, &request) {
        Ok(response) => Ok(response),
        Err(error) if ripgrep_missing(&error) => search_project_with_walk(project_root, request),
        Err(error) => Err(error),
    }
}

fn validate_regex_pattern(request: &SearchRequest) -> Result<()> {
    if matches!(request.match_mode, SearchMatchMode::Regex) {
        Regex::new(&request.pattern).map_err(|error| {
            anyhow::anyhow!("invalid regex pattern `{}`: {error}", request.pattern)
        })?;
    }
    Ok(())
}

fn search_project_with_ripgrep(
    project_root: &Path,
    request: &SearchRequest,
) -> Result<SearchResponse> {
    let indexed_files = count_indexed_files_with_ripgrep(project_root, request.language)?;
    let (matches, truncated) = collect_matches_with_ripgrep(project_root, request)?;
    Ok(SearchResponse {
        pattern: request.pattern.clone(),
        match_mode: request.match_mode,
        indexed_files,
        truncated,
        matches,
    })
}

fn count_indexed_files_with_ripgrep(
    project_root: &Path,
    language: Option<SupportedLanguage>,
) -> Result<usize> {
    let mut command = base_ripgrep_command(project_root);
    command.arg("--files").arg("--null");
    add_language_globs(&mut command, language);
    command.arg("--").arg(".");

    let output = command
        .output()
        .with_context(|| format!("run `rg --files` in {}", project_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "ripgrep file enumeration failed in {}: {stderr}",
            project_root.display()
        );
    }

    Ok(output.stdout.split(|byte| *byte == b'\0').count() - 1)
}

fn collect_matches_with_ripgrep(
    project_root: &Path,
    request: &SearchRequest,
) -> Result<(Vec<SearchMatch>, bool)> {
    let limit = request.max_results.max(1);
    let mut command = base_ripgrep_command(project_root);
    command.arg("--json").arg("--line-number");
    add_language_globs(&mut command, request.language);
    match request.match_mode {
        SearchMatchMode::Literal => {
            command.arg("--fixed-strings");
        }
        SearchMatchMode::Regex => {}
    }
    command.arg("-e").arg(&request.pattern).arg("--").arg(".");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("spawn ripgrep search in {}", project_root.display()))?;

    let stdout = child
        .stdout
        .take()
        .context("ripgrep stdout should be piped")?;
    let mut matches = Vec::new();
    let mut truncated = false;

    {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line.context("read ripgrep json line")?;
            if let Some(search_match) = parse_ripgrep_match_line(&line)? {
                matches.push(search_match);
                if matches.len() >= limit {
                    truncated = true;
                    let _ = child.kill();
                    break;
                }
            }
        }
    }

    let status = child.wait().context("wait for ripgrep search process")?;
    if truncated {
        return Ok((matches, true));
    }

    if status.success() || status.code() == Some(1) {
        return Ok((matches, false));
    }

    let stderr = child
        .stderr
        .take()
        .map(|stderr| {
            let mut stderr = BufReader::new(stderr);
            let mut output = String::new();
            std::io::Read::read_to_string(&mut stderr, &mut output)?;
            Ok::<String, std::io::Error>(output)
        })
        .transpose()
        .context("read ripgrep stderr")?
        .unwrap_or_default();
    anyhow::bail!(
        "ripgrep search failed in {}: {stderr}",
        project_root.display()
    );
}

fn parse_ripgrep_match_line(line: &str) -> Result<Option<SearchMatch>> {
    let payload: Value = serde_json::from_str(line).context("parse ripgrep json payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("match") {
        return Ok(None);
    }

    let data = payload
        .get("data")
        .context("ripgrep match payload missing data object")?;
    let path = data
        .get("path")
        .and_then(json_text_field)
        .context("ripgrep match payload missing path text")?;
    let line_number = data
        .get("line_number")
        .and_then(Value::as_u64)
        .context("ripgrep match payload missing line_number")?;
    let content = data
        .get("lines")
        .and_then(json_text_field)
        .map(|text| text.trim().to_string())
        .context("ripgrep match payload missing line text")?;

    Ok(Some(SearchMatch {
        path: path.to_string(),
        line: line_number as usize,
        content,
    }))
}

fn json_text_field(value: &Value) -> Option<&str> {
    value.get("text").and_then(Value::as_str)
}

fn base_ripgrep_command(project_root: &Path) -> Command {
    let mut command = Command::new("rg");
    command
        .current_dir(project_root)
        .arg("--hidden")
        .arg("--color")
        .arg("never")
        .arg("--no-messages");
    sanitize_invalid_ripgrep_config_path(&mut command);
    command
}

fn sanitize_invalid_ripgrep_config_path(command: &mut Command) {
    let Some(path) = std::env::var_os("RIPGREP_CONFIG_PATH") else {
        return;
    };

    if !path.is_empty() && !Path::new(&path).exists() {
        command.env_remove("RIPGREP_CONFIG_PATH");
    }
}

fn add_language_globs(command: &mut Command, language: Option<SupportedLanguage>) {
    for glob in language_globs(language) {
        command.arg("--glob").arg(glob);
    }
}

fn language_globs(language: Option<SupportedLanguage>) -> &'static [&'static str] {
    match language {
        Some(SupportedLanguage::C) => &["*.c", "*.h"],
        Some(SupportedLanguage::Cpp) => &["*.cpp", "*.cc", "*.cxx", "*.hpp", "*.hh", "*.hxx"],
        Some(SupportedLanguage::CSharp) => &["*.cs"],
        Some(SupportedLanguage::Go) => &["*.go"],
        Some(SupportedLanguage::Java) => &["*.java"],
        Some(SupportedLanguage::JavaScript) => &["*.js", "*.jsx", "*.mjs", "*.cjs"],
        Some(SupportedLanguage::Kotlin) => &["*.kt", "*.kts"],
        Some(SupportedLanguage::Lua) => &["*.lua"],
        Some(SupportedLanguage::Luau) => &["*.luau"],
        Some(SupportedLanguage::Php) => &["*.php"],
        Some(SupportedLanguage::Python) => &["*.py"],
        Some(SupportedLanguage::Ruby) => &["*.rb"],
        Some(SupportedLanguage::Rust) => &["*.rs"],
        Some(SupportedLanguage::Swift) => &["*.swift"],
        Some(SupportedLanguage::TypeScript) => &["*.ts", "*.tsx"],
        Some(SupportedLanguage::Zig) => &["*.zig"],
        None => ALL_LANGUAGE_GLOBS,
    }
}

fn ripgrep_missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    })
}

fn search_project_with_walk(project_root: &Path, request: SearchRequest) -> Result<SearchResponse> {
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
    use super::language_globs;
    use super::search_project;
    use crate::api::SearchMatchMode;
    use crate::api::SearchRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeSet;
    use std::process::Command;
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

    #[test]
    fn search_project_marks_truncated_once_limit_is_hit() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        let contents = (0..150)
            .map(|index| format!("let value_{index} = important_symbol;\n"))
            .collect::<String>();
        std::fs::write(tempdir.path().join("src/lib.rs"), contents).expect("fixture should write");

        let response = search_project(
            tempdir.path(),
            SearchRequest {
                pattern: "important_symbol".to_string(),
                match_mode: SearchMatchMode::Literal,
                language: Some(SupportedLanguage::Rust),
                max_results: 100,
            },
        )
        .expect("search should succeed");

        assert_eq!(response.matches.len(), 100);
        assert_eq!(response.truncated, true);
    }

    #[test]
    fn language_globs_cover_supported_languages_without_duplicates() {
        let unique_globs = language_globs(None)
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();

        assert_eq!(unique_globs.len(), language_globs(None).len());
        assert_eq!(language_globs(Some(SupportedLanguage::Rust)), ["*.rs"]);
        assert_eq!(
            language_globs(Some(SupportedLanguage::TypeScript)),
            ["*.ts", "*.tsx"]
        );
    }

    #[test]
    fn search_tests_assume_ripgrep_is_available() {
        let available = Command::new("rg")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());

        assert!(
            available,
            "ripgrep should be available in native-tldr tests"
        );
    }
}
