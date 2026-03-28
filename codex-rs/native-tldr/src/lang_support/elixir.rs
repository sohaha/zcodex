use tree_sitter::Language;
use tree_sitter::Parser;
use tree_sitter_elixir::LANGUAGE as ELIXIR;
use tree_sitter_language::LanguageFn;

use crate::lang_support::ParserInitError;

const SAMPLE: &str = "defmodule Sample do\n  def hello, do: :ok\nend\n";

pub fn language() -> Language {
    let lang_fn: LanguageFn = ELIXIR;
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
