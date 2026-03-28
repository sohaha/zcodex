use tree_sitter::Language;
use tree_sitter::Parser;
use tree_sitter_language::LanguageFn;
use tree_sitter_swift::LANGUAGE as SWIFT;

use crate::lang_support::ParserInitError;

const SAMPLE: &str = "import Foundation\nfunc main() {}\n";

pub fn language() -> Language {
    let lang_fn: LanguageFn = SWIFT;
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
