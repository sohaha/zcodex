use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use tree_sitter::Parser;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SupportedLanguage {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Php,
    Zig,
}

impl SupportedLanguage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Go => "go",
            Self::Php => "php",
            Self::Zig => "zig",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupportLevel {
    StructureOnly,
    ControlFlow,
    DataFlow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageSupport {
    pub language: SupportedLanguage,
    pub support_level: SupportLevel,
    pub fallback_strategy: &'static str,
}

static LANGUAGE_SUPPORT: Lazy<BTreeMap<&'static str, LanguageSupport>> = Lazy::new(|| {
    [
        LanguageSupport {
            language: SupportedLanguage::Rust,
            support_level: SupportLevel::DataFlow,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::TypeScript,
            support_level: SupportLevel::DataFlow,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::JavaScript,
            support_level: SupportLevel::ControlFlow,
            fallback_strategy: "structure + imports + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Python,
            support_level: SupportLevel::DataFlow,
            fallback_strategy: "structure + search",
        },
        LanguageSupport {
            language: SupportedLanguage::Go,
            support_level: SupportLevel::ControlFlow,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::Php,
            support_level: SupportLevel::ControlFlow,
            fallback_strategy: "structure + imports",
        },
        LanguageSupport {
            language: SupportedLanguage::Zig,
            support_level: SupportLevel::StructureOnly,
            fallback_strategy: "structure + search",
        },
    ]
    .into_iter()
    .map(|support| (support.language.as_str(), support))
    .collect()
});

#[derive(Debug, Default)]
pub struct LanguageRegistry;

impl LanguageRegistry {
    pub fn support_for(language: SupportedLanguage) -> &'static LanguageSupport {
        LANGUAGE_SUPPORT
            .get(language.as_str())
            .expect("supported languages must exist in the registry")
    }

    pub fn parser_for(&self, _language: SupportedLanguage) -> Parser {
        Parser::new()
    }

    pub fn supported_languages(&self) -> Vec<SupportedLanguage> {
        LANGUAGE_SUPPORT
            .values()
            .map(|support| support.language)
            .collect()
    }
}
