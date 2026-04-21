#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ZtokBehavior {
    #[default]
    Enhanced,
    Basic,
}

impl ZtokBehavior {
    pub(crate) fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("basic") => Self::Basic,
            _ => Self::Enhanced,
        }
    }

    pub(crate) const fn is_basic(self) -> bool {
        matches!(self, Self::Basic)
    }
}

pub const ZTOK_BEHAVIOR_ENV_VAR: &str = "CODEX_ZTOK_BEHAVIOR";
