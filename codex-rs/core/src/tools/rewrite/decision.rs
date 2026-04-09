use crate::tools::rewrite::tldr_routing::SearchSignal;
use crate::tools::router::ToolCall;
use codex_native_tldr::tool_api::TldrToolAction;

#[derive(Debug, Clone)]
pub(crate) enum ToolRewriteDecision {
    Passthrough {
        call: ToolCall,
        reason: &'static str,
        signal: Option<SearchSignal>,
    },
    Rewrite {
        call: ToolCall,
        reason: &'static str,
        action: Option<TldrToolAction>,
        signal: Option<SearchSignal>,
    },
}

impl ToolRewriteDecision {
    pub(crate) fn call(&self) -> &ToolCall {
        match self {
            Self::Passthrough { call, .. } | Self::Rewrite { call, .. } => call,
        }
    }

    pub(crate) fn into_call(self) -> ToolCall {
        match self {
            Self::Passthrough { call, .. } | Self::Rewrite { call, .. } => call,
        }
    }

    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::Passthrough { reason, .. } | Self::Rewrite { reason, .. } => reason,
        }
    }

    pub(crate) fn action(&self) -> Option<&TldrToolAction> {
        match self {
            Self::Rewrite { action, .. } => action.as_ref(),
            Self::Passthrough { .. } => None,
        }
    }

    pub(crate) fn signal(&self) -> Option<SearchSignal> {
        match self {
            Self::Passthrough { signal, .. } | Self::Rewrite { signal, .. } => *signal,
        }
    }
}
