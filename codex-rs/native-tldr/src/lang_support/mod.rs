mod c;
mod cpp;
mod csharp;
mod elixir;
mod go;
mod java;
mod javascript;
mod lua;
mod luau;
mod php;
mod python;
mod ruby;
mod rust;
mod scala;
mod swift;
mod typescript;
mod zig;

use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;
use tree_sitter::Parser;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SupportedLanguage {
    C,
    Cpp,
    CSharp,
    Elixir,
    Go,
    Java,
    JavaScript,
    Lua,
    Luau,
    Php,
    Python,
    Ruby,
    Rust,
    Scala,
    Swift,
    TypeScript,
    Zig,
}

impl SupportedLanguage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Elixir => "elixir",
            Self::Go => "go",
            Self::Java => "java",
            Self::JavaScript => "javascript",
            Self::Lua => "lua",
            Self::Luau => "luau",
            Self::Php => "php",
            Self::Python => "python",
            Self::Ruby => "ruby",
            Self::Rust => "rust",
            Self::Scala => "scala",
            Self::Swift => "swift",
            Self::TypeScript => "typescript",
            Self::Zig => "zig",
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|value| value.to_str()) {
            Some("c") | Some("h") => Some(Self::C),
            Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("hh") | Some("hxx") => {
                Some(Self::Cpp)
            }
            Some("cs") => Some(Self::CSharp),
            Some("ex") | Some("exs") => Some(Self::Elixir),
            Some("rs") => Some(Self::Rust),
            Some("ts" | "tsx") => Some(Self::TypeScript),
            Some("js" | "jsx" | "mjs" | "cjs") => Some(Self::JavaScript),
            Some("py") => Some(Self::Python),
            Some("go") => Some(Self::Go),
            Some("php") => Some(Self::Php),
            Some("scala") => Some(Self::Scala),
            Some("swift") => Some(Self::Swift),
            Some("lua") => Some(Self::Lua),
            Some("luau") => Some(Self::Luau),
            Some("java") => Some(Self::Java),
            Some("rb") => Some(Self::Ruby),
            Some("zig") => Some(Self::Zig),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupportLevel {
    StructureOnly,
    ControlFlow,
    DataFlow,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolExtractorKind {
    Dedicated,
    Heuristic,
}

impl SymbolExtractorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dedicated => "dedicated",
            Self::Heuristic => "heuristic",
        }
    }

    pub const fn uses_dedicated_extractor(self) -> bool {
        matches!(self, Self::Dedicated)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolRelationshipSupport {
    Precise,
    Heuristic,
}

impl SymbolRelationshipSupport {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Precise => "precise",
            Self::Heuristic => "heuristic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageSupport {
    pub language: SupportedLanguage,
    pub support_level: SupportLevel,
    pub symbol_extractor: SymbolExtractorKind,
    pub symbol_relationship_support: SymbolRelationshipSupport,
    pub fallback_strategy: &'static str,
}

static LANGUAGE_SUPPORT: Lazy<BTreeMap<&'static str, LanguageSupport>> = Lazy::new(|| {
    [
        LanguageSupport {
            language: SupportedLanguage::C,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::Cpp,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::CSharp,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::Elixir,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Go,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::Java,
            support_level: SupportLevel::DataFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::JavaScript,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Lua,
            support_level: SupportLevel::StructureOnly,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Luau,
            support_level: SupportLevel::StructureOnly,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Php,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::Python,
            support_level: SupportLevel::DataFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Ruby,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Rust,
            support_level: SupportLevel::DataFlow,
            symbol_extractor: SymbolExtractorKind::Dedicated,
            symbol_relationship_support: SymbolRelationshipSupport::Precise,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Scala,
            support_level: SupportLevel::ControlFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Swift,
            support_level: SupportLevel::DataFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::TypeScript,
            support_level: SupportLevel::DataFlow,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Zig,
            support_level: SupportLevel::StructureOnly,
            symbol_extractor: SymbolExtractorKind::Heuristic,
            symbol_relationship_support: SymbolRelationshipSupport::Heuristic,
            fallback_strategy: "structure + search",
        },
    ]
    .into_iter()
    .map(|support| (support.language.as_str(), support))
    .collect()
});

#[derive(Debug, Default)]
pub struct LanguageRegistry;

#[derive(Debug, Error)]
pub enum ParserInitError {
    #[error("unsupported language: {language}")]
    UnsupportedLanguage { language: &'static str },
    #[error("failed to set tree-sitter language: {0}")]
    SetLanguage(tree_sitter::LanguageError),
}

impl LanguageRegistry {
    pub fn support_for(language: SupportedLanguage) -> &'static LanguageSupport {
        let language_name = language.as_str();
        let Some(support) = LANGUAGE_SUPPORT.get(language_name) else {
            unreachable!("supported languages must exist in the registry: {language_name}");
        };

        support
    }

    pub fn parser_for(&self, language: SupportedLanguage) -> Result<Parser, ParserInitError> {
        match language {
            SupportedLanguage::C => c::parser(),
            SupportedLanguage::Cpp => cpp::parser(),
            SupportedLanguage::CSharp => csharp::parser(),
            SupportedLanguage::Elixir => elixir::parser(),
            SupportedLanguage::Rust => rust::parser(),
            SupportedLanguage::TypeScript => typescript::parser(),
            SupportedLanguage::JavaScript => javascript::parser(),
            SupportedLanguage::Python => python::parser(),
            SupportedLanguage::Go => go::parser(),
            SupportedLanguage::Java => java::parser(),
            SupportedLanguage::Lua => lua::parser(),
            SupportedLanguage::Luau => luau::parser(),
            SupportedLanguage::Ruby => ruby::parser(),
            SupportedLanguage::Php => php::parser(),
            SupportedLanguage::Scala => scala::parser(),
            SupportedLanguage::Swift => swift::parser(),
            SupportedLanguage::Zig => zig::parser(),
        }
    }

    pub fn supported_languages(&self) -> Vec<SupportedLanguage> {
        LANGUAGE_SUPPORT
            .values()
            .map(|support| support.language)
            .collect()
    }

    pub fn sample_for(&self, language: SupportedLanguage) -> &'static str {
        match language {
            SupportedLanguage::C => c::sample(),
            SupportedLanguage::Cpp => cpp::sample(),
            SupportedLanguage::CSharp => csharp::sample(),
            SupportedLanguage::Elixir => elixir::sample(),
            SupportedLanguage::Rust => rust::sample(),
            SupportedLanguage::TypeScript => typescript::sample(),
            SupportedLanguage::JavaScript => javascript::sample(),
            SupportedLanguage::Python => python::sample(),
            SupportedLanguage::Go => go::sample(),
            SupportedLanguage::Java => java::sample(),
            SupportedLanguage::Lua => lua::sample(),
            SupportedLanguage::Luau => luau::sample(),
            SupportedLanguage::Ruby => ruby::sample(),
            SupportedLanguage::Php => php::sample(),
            SupportedLanguage::Scala => scala::sample(),
            SupportedLanguage::Swift => swift::sample(),
            SupportedLanguage::Zig => zig::sample(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LanguageRegistry;
    use super::SupportedLanguage;
    use super::SymbolExtractorKind;
    use super::SymbolRelationshipSupport;

    #[test]
    fn rust_is_the_only_language_with_dedicated_symbol_extraction_today() {
        let registry = LanguageRegistry;
        let dedicated = registry
            .supported_languages()
            .into_iter()
            .filter(|language| {
                LanguageRegistry::support_for(*language)
                    .symbol_extractor
                    .uses_dedicated_extractor()
            })
            .collect::<Vec<_>>();

        assert_eq!(dedicated, vec![SupportedLanguage::Rust]);
    }

    #[test]
    fn non_rust_languages_are_marked_as_heuristic_relationship_support() {
        let registry = LanguageRegistry;

        for language in registry.supported_languages() {
            let support = LanguageRegistry::support_for(language);
            if language == SupportedLanguage::Rust {
                assert_eq!(support.symbol_extractor, SymbolExtractorKind::Dedicated);
                assert_eq!(
                    support.symbol_relationship_support,
                    SymbolRelationshipSupport::Precise
                );
            } else {
                assert_eq!(support.symbol_extractor, SymbolExtractorKind::Heuristic);
                assert_eq!(
                    support.symbol_relationship_support,
                    SymbolRelationshipSupport::Heuristic
                );
            }
        }
    }
}
