use tree_sitter::Language;
use tree_sitter::Parser;
use tree_sitter_language::LanguageFn;
use tree_sitter_typescript::LANGUAGE_TYPESCRIPT as TYPESCRIPT;

use crate::lang_support::ParserInitError;

const SAMPLE: &str = "export function main(): number { return 1; }\n";

pub fn language() -> Language {
    let lang_fn: LanguageFn = TYPESCRIPT;
    lang_fn.into()
}

pub fn parser() -> Result<Parser, ParserInitError> {
    let mut parser = Parser::new();
    parser
        .set_language(&language())
        .map_err(ParserInitError::SetLanguage)?;
    Ok(parser)
}

pub fn sample() -> &'static str {
    SAMPLE
}
