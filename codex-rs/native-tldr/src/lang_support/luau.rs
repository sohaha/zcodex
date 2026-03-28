use tree_sitter::Language;
use tree_sitter::Parser;
use tree_sitter_language::LanguageFn;
use tree_sitter_luau::LANGUAGE as LUAU;

use crate::lang_support::ParserInitError;

const SAMPLE: &str = "function main()\nend\n";

pub fn language() -> Language {
    let lang_fn: LanguageFn = LUAU;
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
