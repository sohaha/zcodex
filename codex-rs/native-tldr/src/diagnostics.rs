use crate::api::DiagnosticItem;
use crate::api::DiagnosticSeverity;
use crate::api::DiagnosticToolStatus;
use crate::api::DiagnosticsRequest;
use crate::api::DiagnosticsResponse;
use crate::api::DoctorRequest;
use crate::api::DoctorResponse;
use crate::lang_support::SupportedLanguage;
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use which::which;

pub(crate) fn collect_diagnostics(
    project_root: &Path,
    request: DiagnosticsRequest,
) -> Result<DiagnosticsResponse> {
    let target_path = normalize_request_path(project_root, &request.path);
    let runners = language_runners(request.language, &target_path);
    let mut tools = Vec::new();
    let mut diagnostics = Vec::new();

    for runner in runners {
        if !request.only_tools.is_empty()
            && !request.only_tools.iter().any(|tool| tool == runner.name)
        {
            continue;
        }
        if runner.is_lint && !request.run_lint {
            continue;
        }
        if runner.is_typecheck && !request.run_typecheck {
            continue;
        }
        let available = which(runner.binary).is_ok();
        tools.push(DiagnosticToolStatus {
            tool: runner.name.to_string(),
            available,
        });
        if !available {
            continue;
        }
        let output = Command::new(runner.binary)
            .args(runner.args.iter())
            .current_dir(project_root)
            .output()
            .with_context(|| format!("run {}", runner.binary))?;
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        diagnostics.push(DiagnosticItem {
            path: request.path.clone(),
            line: 1,
            column: 1,
            severity: DiagnosticSeverity::Error,
            message,
            code: None,
            source: runner.name.to_string(),
        });
        if diagnostics.len() >= request.max_issues.max(1) {
            break;
        }
    }

    let message = if diagnostics.is_empty() {
        "diagnostics completed without reported errors".to_string()
    } else {
        format!("diagnostics reported {} issues", diagnostics.len())
    };

    Ok(DiagnosticsResponse {
        language: request.language,
        path: request.path,
        tools,
        diagnostics,
        message,
    })
}

pub(crate) fn doctor_tools(request: DoctorRequest) -> DoctorResponse {
    let mut tools = [
        "cargo",
        "cargo-clippy",
        "python",
        "python3",
        "pyright",
        "ruff",
        "node",
        "tsc",
        "go",
        "golangci-lint",
        "php",
        "phpstan",
        "ruby",
        "rubocop",
        "javac",
        "ktlint",
        "swift",
        "swiftlint",
        "cppcheck",
        "elixir",
        "mix",
    ]
    .into_iter()
    .map(|tool| DiagnosticToolStatus {
        tool: tool.to_string(),
        available: which(tool).is_ok(),
    })
    .collect::<Vec<_>>();
    if !request.only_tools.is_empty() {
        tools.retain(|tool| request.only_tools.iter().any(|name| name == &tool.tool));
    }

    let available = tools.iter().filter(|tool| tool.available).count();
    let install_hint = if request.include_install_hints && available < tools.len() {
        "; install missing linters/typecheckers to improve diagnostics"
    } else {
        ""
    };
    DoctorResponse {
        tools,
        message: format!("doctor found {available} available tools{install_hint}"),
    }
}

struct DiagnosticRunner {
    name: &'static str,
    binary: &'static str,
    args: Vec<String>,
    is_lint: bool,
    is_typecheck: bool,
}

fn language_runners(language: SupportedLanguage, path: &Path) -> Vec<DiagnosticRunner> {
    let path = path.display().to_string();
    match language {
        SupportedLanguage::Rust => vec![
            DiagnosticRunner {
                name: "cargo-check",
                binary: "cargo",
                args: vec!["check".to_string(), "--message-format=short".to_string()],
                is_lint: false,
                is_typecheck: true,
            },
            DiagnosticRunner {
                name: "cargo-clippy",
                binary: "cargo",
                args: vec!["clippy".to_string(), "--message-format=short".to_string()],
                is_lint: true,
                is_typecheck: false,
            },
        ],
        SupportedLanguage::Python => vec![
            DiagnosticRunner {
                name: "pyright",
                binary: "pyright",
                args: vec![path.clone()],
                is_lint: false,
                is_typecheck: true,
            },
            DiagnosticRunner {
                name: "ruff",
                binary: "ruff",
                args: vec!["check".to_string(), path],
                is_lint: true,
                is_typecheck: false,
            },
        ],
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => vec![DiagnosticRunner {
            name: "tsc",
            binary: "tsc",
            args: vec!["--noEmit".to_string(), path],
            is_lint: false,
            is_typecheck: true,
        }],
        SupportedLanguage::Go => vec![DiagnosticRunner {
            name: "go-vet",
            binary: "go",
            args: vec!["vet".to_string(), "./...".to_string()],
            is_lint: false,
            is_typecheck: true,
        }],
        SupportedLanguage::Php => vec![DiagnosticRunner {
            name: "phpstan",
            binary: "phpstan",
            args: vec!["analyse".to_string(), path],
            is_lint: false,
            is_typecheck: true,
        }],
        _ => Vec::new(),
    }
}

fn normalize_request_path(project_root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::doctor_tools;
    use crate::api::DoctorRequest;

    #[test]
    fn doctor_reports_known_tool_entries() {
        let response = doctor_tools(DoctorRequest {
            language: None,
            only_tools: Vec::new(),
            include_install_hints: true,
        });
        assert!(response.tools.iter().any(|tool| tool.tool == "cargo"));
        assert!(response.tools.iter().any(|tool| tool.tool == "python3"));
    }
}
