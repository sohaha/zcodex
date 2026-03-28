use tree_sitter::Language;
use tree_sitter::Parser;
use tree_sitter_language::LanguageFn;
use tree_sitter_ruby::LANGUAGE as RUBY;

use crate::lang_support::ParserInitError;

const SAMPLE: &str = "def main\nend\n";

pub fn language() -> Language {
    let lang_fn: LanguageFn = RUBY;
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
