use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use crate::semantic::EmbeddingUnit;
use anyhow::Context;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;
use tree_sitter::Node;

const PREVIEW_LINES: usize = 12;
const CONTROL_FLOW_KINDS: &[&str] = &["if_expression", "if_let_expression", "match_expression"];
const LOOP_KINDS: &[&str] = &["for_expression", "while_expression", "loop_expression"];
const RETURN_KINDS: &[&str] = &["return_expression"];
const AWAIT_KINDS: &[&str] = &["await_expression"];
const ASSIGNMENT_KINDS: &[&str] = &["assignment_expression", "compound_assignment_expr"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ControlFlowFacts {
    branches: usize,
    loops: usize,
    returns: usize,
    awaits: usize,
    outgoing_calls: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DataFlowFacts {
    parameters: usize,
    locals: usize,
    mutable_bindings: usize,
    assignments: usize,
    references: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RustSymbolRecord {
    path: PathBuf,
    symbol: String,
    qualified_symbol: Option<String>,
    symbol_aliases: Vec<String>,
    kind: String,
    line: usize,
    span_end_line: usize,
    module_path: Vec<String>,
    visibility: Option<String>,
    signature: Option<String>,
    docs: Vec<String>,
    imports: Vec<String>,
    references: Vec<String>,
    calls: Vec<String>,
    dependencies: Vec<String>,
    code_preview: String,
    cfg: ControlFlowFacts,
    dfg: DataFlowFacts,
}

#[derive(Debug, Clone)]
struct VisitContext<'a> {
    path: &'a Path,
    source: &'a str,
    lines: &'a [&'a str],
    file_imports: &'a [String],
    file_module_path: &'a [String],
    inline_modules: Vec<String>,
    container: Option<String>,
}

impl<'a> VisitContext<'a> {
    fn full_module_path(&self) -> Vec<String> {
        let mut path = Vec::new();
        for segment in self
            .file_module_path
            .iter()
            .chain(self.inline_modules.iter())
            .cloned()
        {
            if path.last() != Some(&segment) {
                path.push(segment);
            }
        }
        path
    }

    fn with_inline_module(&self, name: String) -> Self {
        let mut inline_modules = self.inline_modules.clone();
        inline_modules.push(name);
        Self {
            path: self.path,
            source: self.source,
            lines: self.lines,
            file_imports: self.file_imports,
            file_module_path: self.file_module_path,
            inline_modules,
            container: self.container.clone(),
        }
    }

    fn with_container(&self, container: Option<String>) -> Self {
        Self {
            path: self.path,
            source: self.source,
            lines: self.lines,
            file_imports: self.file_imports,
            file_module_path: self.file_module_path,
            inline_modules: self.inline_modules.clone(),
            container,
        }
    }
}

pub(crate) fn extract_units(path: &Path, contents: &str) -> Result<Vec<EmbeddingUnit>> {
    let mut parser = LanguageRegistry
        .parser_for(SupportedLanguage::Rust)
        .context("initialize Rust parser")?;
    let Some(tree) = parser.parse(contents, None) else {
        return Ok(Vec::new());
    };
    let root = tree.root_node();
    let lines = contents.lines().collect::<Vec<_>>();
    let imports = collect_use_statements(root, contents);
    let file_module_path = file_module_path(path);
    let context = VisitContext {
        path,
        source: contents,
        lines: &lines,
        file_imports: &imports,
        file_module_path: &file_module_path,
        inline_modules: Vec::new(),
        container: None,
    };
    let mut records = Vec::new();
    collect_symbols(root, &context, &mut records);
    Ok(records.into_iter().map(EmbeddingUnit::from).collect())
}

fn collect_symbols(
    node: Node<'_>,
    context: &VisitContext<'_>,
    records: &mut Vec<RustSymbolRecord>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                if let Some(record) = build_record(child, context, "function") {
                    records.push(record);
                }
            }
            "struct_item" => {
                if let Some(record) = build_record(child, context, "struct") {
                    records.push(record);
                }
            }
            "enum_item" => {
                if let Some(record) = build_record(child, context, "enum") {
                    records.push(record);
                }
            }
            "trait_item" => {
                let trait_name = symbol_name(child, context.source);
                if let Some(record) = build_record(child, context, "trait") {
                    records.push(record);
                }
                if let Some(name) = trait_name
                    && let Some(body) = declaration_list(child)
                {
                    let next = context.with_container(Some(name));
                    collect_symbols(body, &next, records);
                }
            }
            "impl_item" => {
                let next = context.with_container(impl_container(child, context.source));
                if let Some(body) = declaration_list(child) {
                    collect_symbols(body, &next, records);
                }
            }
            "mod_item" => {
                let module_name = symbol_name(child, context.source);
                if let Some(record) = build_record(child, context, "module") {
                    records.push(record);
                }
                if let Some(name) = module_name
                    && let Some(body) = declaration_list(child)
                {
                    let next = context.with_inline_module(name);
                    collect_symbols(body, &next, records);
                }
            }
            "const_item" => {
                if let Some(record) = build_record(child, context, "const") {
                    records.push(record);
                }
            }
            "type_item" => {
                if let Some(record) = build_record(child, context, "type_alias") {
                    records.push(record);
                }
            }
            _ => {}
        }
    }
}

fn build_record(
    node: Node<'_>,
    context: &VisitContext<'_>,
    default_kind: &str,
) -> Option<RustSymbolRecord> {
    let symbol = symbol_name(node, context.source)?;
    let module_path = context.full_module_path();
    let line = node.start_position().row + 1;
    let span_end_line = node.end_position().row + 1;
    let signature = signature_text(node, context.source);
    let code_preview = preview(&node_text(node, context.source), PREVIEW_LINES);
    let visibility = visibility_text(node, context.source);
    let docs = doc_comments(context.lines, line);
    let imports = context.file_imports.to_vec();
    let kind = if context.container.is_some() && default_kind == "function" {
        "method".to_string()
    } else {
        default_kind.to_string()
    };
    let calls = collect_call_names(node, context.source);
    let references = collect_reference_names(node, context.source, &symbol);
    let cfg = control_flow_facts(node, context.source);
    let dfg = data_flow_facts(node, context.source);
    let qualified_symbol = qualified_symbol(&module_path, context.container.as_deref(), &symbol);
    let symbol_aliases = symbol_aliases(
        &symbol,
        qualified_symbol.as_deref(),
        context.container.as_deref(),
        &module_path,
    );
    Some(RustSymbolRecord {
        dependencies: dependency_segments(context.path),
        path: context.path.to_path_buf(),
        symbol,
        qualified_symbol,
        symbol_aliases,
        kind,
        line,
        span_end_line,
        module_path,
        visibility,
        signature,
        docs,
        imports,
        references,
        calls,
        code_preview,
        cfg,
        dfg,
    })
}

fn collect_use_statements(root: Node<'_>, source: &str) -> Vec<String> {
    let mut uses = Vec::new();
    walk_named(root, |node| {
        if node.kind() != "use_declaration" {
            return;
        }
        let text = compact_whitespace(&node_text(node, source));
        if !text.is_empty() && !uses.iter().any(|existing| existing == &text) {
            uses.push(text);
        }
    });
    uses
}

fn impl_container(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("type")
        .and_then(|value| trimmed_node_text(value, source))
        .map(|value| normalize_container_name(&value))
}

fn declaration_list(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("body").or_else(|| {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .find(|child| child.kind() == "declaration_list")
    })
}

fn symbol_name(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|name| trimmed_node_text(name, source))
        .or_else(|| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find(|child| matches!(child.kind(), "identifier" | "type_identifier"))
                .and_then(|child| trimmed_node_text(child, source))
        })
}

fn signature_text(node: Node<'_>, source: &str) -> Option<String> {
    let body = node.child_by_field_name("body");
    let raw = if let Some(body) = body {
        source.get(node.start_byte()..body.start_byte())?
    } else {
        source.get(node.start_byte()..node.end_byte())?
    };
    let head = raw
        .split('{')
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_end_matches(';');
    let compact = compact_whitespace(head);
    (!compact.is_empty()).then_some(compact)
}

fn visibility_text(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("visibility")
        .and_then(|value| trimmed_node_text(value, source))
        .or_else(|| {
            signature_text(node, source)
                .and_then(|text| text.starts_with("pub").then(|| "pub".to_string()))
        })
}

fn doc_comments(lines: &[&str], line: usize) -> Vec<String> {
    let mut docs = Vec::new();
    let mut index = line.saturating_sub(1);
    while index > 0 {
        let current = lines[index - 1].trim();
        if current.starts_with("#[") {
            index -= 1;
            continue;
        }
        if let Some(value) = current
            .strip_prefix("///")
            .or_else(|| current.strip_prefix("//!"))
        {
            docs.push(value.trim().to_string());
            index -= 1;
            continue;
        }
        break;
    }
    docs.reverse();
    docs
}

fn collect_call_names(node: Node<'_>, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    walk_named(node, |current| match current.kind() {
        "call_expression" => {
            if let Some(function) = current.child_by_field_name("function")
                && let Some(name) = callable_name(function, source)
            {
                push_unique(&mut names, name);
            }
        }
        "macro_invocation" => {
            if let Some(name) = macro_name(current, source) {
                push_unique(&mut names, name);
            }
        }
        _ => {}
    });
    names
}

fn collect_reference_names(node: Node<'_>, source: &str, symbol: &str) -> Vec<String> {
    let mut names = Vec::new();
    walk_named(node, |current| match current.kind() {
        "type_identifier" | "scoped_identifier" | "scoped_type_identifier" => {
            if let Some(text) = trimmed_node_text(current, source) {
                for token in identifier_tokens(&text) {
                    if token != symbol {
                        push_unique(&mut names, token);
                    }
                }
            }
        }
        _ => {}
    });
    names
}

fn control_flow_facts(node: Node<'_>, source: &str) -> ControlFlowFacts {
    let mut facts = ControlFlowFacts::default();
    walk_named(node, |current| {
        let kind = current.kind();
        if CONTROL_FLOW_KINDS.contains(&kind) {
            facts.branches += 1;
        }
        if LOOP_KINDS.contains(&kind) {
            facts.loops += 1;
        }
        if RETURN_KINDS.contains(&kind) {
            facts.returns += 1;
        }
        if AWAIT_KINDS.contains(&kind) {
            facts.awaits += 1;
        }
        if kind == "call_expression" {
            facts.outgoing_calls += 1;
        }
        if kind == "macro_invocation" && macro_name(current, source).is_some() {
            facts.outgoing_calls += 1;
        }
    });
    facts
}

fn data_flow_facts(node: Node<'_>, source: &str) -> DataFlowFacts {
    let mut facts = DataFlowFacts::default();
    if let Some(parameters) = node.child_by_field_name("parameters") {
        facts.parameters = collect_bindings(parameters, source).len();
    }
    walk_named(node, |current| {
        let kind = current.kind();
        if kind == "let_declaration" {
            let bindings = collect_bindings(current, source);
            facts.locals += bindings.len();
            if node_text(current, source).contains("mut ") {
                facts.mutable_bindings += bindings.len();
            }
        }
        if ASSIGNMENT_KINDS.contains(&kind) {
            facts.assignments += 1;
        }
        if matches!(
            kind,
            "identifier" | "type_identifier" | "field_identifier" | "scoped_identifier"
        ) {
            facts.references += 1;
        }
    });
    facts
}

fn collect_bindings(node: Node<'_>, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    walk_named(node, |current| {
        if current.kind() == "self_parameter" {
            push_unique(&mut names, "self".to_string());
            return;
        }
        if current.kind() == "identifier"
            && let Some(parent) = current.parent()
            && matches!(
                parent.kind(),
                "parameter"
                    | "self_parameter"
                    | "identifier"
                    | "let_declaration"
                    | "mutable_specifier"
                    | "tuple_pattern"
                    | "tuple_struct_pattern"
                    | "struct_pattern"
                    | "reference_pattern"
            )
            && let Some(name) = trimmed_node_text(current, source)
        {
            push_unique(&mut names, name);
        }
    });
    names
}

fn callable_name(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" | "field_identifier" => trimmed_node_text(node, source),
        "generic_function" => node
            .child_by_field_name("function")
            .and_then(|child| callable_name(child, source))
            .or_else(|| {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .find_map(|child| callable_name(child, source))
            }),
        "field_expression" => node
            .child_by_field_name("field")
            .and_then(|child| trimmed_node_text(child, source))
            .or_else(|| last_symbol_segment(&node_text(node, source))),
        "scoped_identifier" | "scoped_type_identifier" => {
            scoped_symbol_name(&node_text(node, source))
                .or_else(|| last_symbol_segment(&node_text(node, source)))
        }
        _ => last_symbol_segment(&node_text(node, source)),
    }
}

fn macro_name(node: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "identifier" | "scoped_identifier"))
        .and_then(|child| callable_name(child, source))
}

fn qualified_symbol(
    module_path: &[String],
    container: Option<&str>,
    symbol: &str,
) -> Option<String> {
    let mut parts = Vec::new();
    parts.extend(module_path.iter().cloned());
    if let Some(container) = container {
        parts.push(container.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        parts.push(symbol.to_string());
        Some(parts.join("::"))
    }
}

fn symbol_aliases(
    symbol: &str,
    qualified: Option<&str>,
    container: Option<&str>,
    module_path: &[String],
) -> Vec<String> {
    let mut aliases = Vec::new();
    push_unique(&mut aliases, symbol.to_string());
    if let Some(container) = container {
        push_unique(&mut aliases, format!("{container}::{symbol}"));
        if !module_path.is_empty() {
            push_unique(
                &mut aliases,
                format!("{}::{container}::{symbol}", module_path.join("::")),
            );
        }
    } else if !module_path.is_empty() {
        push_unique(
            &mut aliases,
            format!("{}::{symbol}", module_path.join("::")),
        );
    }
    if let Some(qualified) = qualified {
        push_unique(&mut aliases, qualified.to_string());
    }
    aliases
}

fn file_module_path(path: &Path) -> Vec<String> {
    let mut segments = path
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect::<Vec<_>>();
    if matches!(segments.first().map(String::as_str), Some("src")) {
        segments.remove(0);
    }
    let Some(last) = segments.pop() else {
        return Vec::new();
    };
    let stem = Path::new(&last)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !matches!(stem, "" | "lib" | "main" | "mod") {
        segments.push(stem.to_string());
    }
    segments
}

fn dependency_segments(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| {
            let value = component.as_os_str().to_str()?;
            (!value.is_empty() && value != ".").then_some(value.to_string())
        })
        .collect()
}

fn compact_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trimmed_node_text(node: Node<'_>, source: &str) -> Option<String> {
    let text = node_text(node, source);
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn node_text(node: Node<'_>, source: &str) -> String {
    node.utf8_text(source.as_bytes())
        .map_or_else(|_| String::new(), ToOwned::to_owned)
}

fn preview(contents: &str, max_lines: usize) -> String {
    contents
        .lines()
        .take(max_lines)
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_container_name(value: &str) -> String {
    compact_whitespace(value)
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .split('<')
        .next()
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn identifier_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
            continue;
        }
        if !current.is_empty() {
            push_unique(&mut tokens, std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        push_unique(&mut tokens, current);
    }
    tokens
}

fn last_symbol_segment(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let head = trimmed.split('<').next().unwrap_or(trimmed);
    let last = head
        .rsplit("::")
        .next()
        .unwrap_or(head)
        .rsplit('.')
        .next()
        .unwrap_or(head)
        .trim()
        .trim_start_matches('&');
    (!last.is_empty()).then(|| last.to_string())
}

fn scoped_symbol_name(text: &str) -> Option<String> {
    let compact = compact_whitespace(text);
    let head = compact
        .split('<')
        .next()
        .unwrap_or(compact.as_str())
        .trim()
        .trim_start_matches("::");
    (!head.is_empty()).then(|| head.to_string())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn walk_named(mut node: Node<'_>, mut visit: impl FnMut(Node<'_>)) {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        visit(current);
        let mut cursor = current.walk();
        let mut children = current.named_children(&mut cursor).collect::<Vec<_>>();
        children.sort_by_key(Node::start_byte);
        while let Some(child) = children.pop() {
            node = child;
            stack.push(node);
        }
    }
}

impl From<RustSymbolRecord> for EmbeddingUnit {
    fn from(record: RustSymbolRecord) -> Self {
        let cfg_summary = if record.kind == "function" || record.kind == "method" {
            format!(
                "branches={}, loops={}, returns={}, awaits={}, outgoing calls={}",
                record.cfg.branches,
                record.cfg.loops,
                record.cfg.returns,
                record.cfg.awaits,
                record.cfg.outgoing_calls
            )
        } else {
            format!(
                "declaration scope; outgoing calls={}",
                record.cfg.outgoing_calls
            )
        };
        let dfg_summary = format!(
            "params={}, locals={}, mutable bindings={}, assignments={}, references={}",
            record.dfg.parameters,
            record.dfg.locals,
            record.dfg.mutable_bindings,
            record.dfg.assignments,
            record.dfg.references
        );
        Self {
            path: record.path,
            language: SupportedLanguage::Rust,
            symbol: Some(record.symbol),
            qualified_symbol: record.qualified_symbol,
            symbol_aliases: record.symbol_aliases,
            kind: record.kind,
            line: record.line,
            span_end_line: record.span_end_line,
            module_path: record.module_path,
            visibility: record.visibility,
            signature: record.signature,
            docs: record.docs,
            imports: record.imports,
            references: record.references,
            code_preview: record.code_preview,
            calls: record.calls,
            called_by: Vec::new(),
            dependencies: record.dependencies,
            cfg_summary,
            dfg_summary,
            embedding_vector: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::extract_units;
    use pretty_assertions::assert_eq;
    use std::path::Path;

    #[test]
    fn rust_units_capture_modules_methods_and_flow_facts() {
        let units = extract_units(
            Path::new("src/auth.rs"),
            r#"
use crate::models::Session;

/// Login orchestration.
pub mod auth {
    pub struct AuthService;

    impl AuthService {
        pub fn login(&self, token: &str) -> Option<Session> {
            if token.is_empty() {
                return None;
            }

            self.validate(token);
            Some(Session::new())
        }

        fn validate(&self, token: &str) {
            let trimmed = token.trim();
            println!("{}", trimmed);
        }
    }
}
"#,
        )
        .expect("rust extraction should succeed");

        let login = units
            .iter()
            .find(|unit| unit.symbol.as_deref() == Some("login"))
            .expect("login symbol should exist");
        assert_eq!(login.kind, "method");
        assert_eq!(
            login.qualified_symbol.as_deref(),
            Some("auth::AuthService::login")
        );
        assert_eq!(login.module_path, vec!["auth".to_string()]);
        assert_eq!(login.visibility.as_deref(), Some("pub"));
        assert!(login.calls.contains(&"validate".to_string()));
        assert!(login.references.contains(&"Session".to_string()));
        assert!(login.cfg_summary.contains("branches=1"));
        assert!(login.dfg_summary.contains("assignments=0"));

        let validate = units
            .iter()
            .find(|unit| unit.symbol.as_deref() == Some("validate"))
            .expect("validate symbol should exist");
        assert_eq!(
            validate.qualified_symbol.as_deref(),
            Some("auth::AuthService::validate")
        );
        assert!(validate.dfg_summary.contains("locals=1"));

        let module = units
            .iter()
            .find(|unit| unit.symbol.as_deref() == Some("auth"))
            .expect("module symbol should exist");
        assert!(
            module
                .docs
                .iter()
                .any(|doc| doc.contains("Login orchestration"))
        );
    }

    #[test]
    fn rust_units_preserve_scoped_call_paths() {
        let units = extract_units(
            Path::new("src/lib.rs"),
            r#"
mod auth {
    pub fn validate() {}
}

fn login() {
    auth::validate();
}
"#,
        )
        .expect("rust extraction should succeed");

        let login = units
            .iter()
            .find(|unit| unit.symbol.as_deref() == Some("login"))
            .expect("login symbol should exist");
        assert!(login.calls.contains(&"auth::validate".to_string()));
        assert!(!login.calls.contains(&"validate".to_string()));
    }
}
