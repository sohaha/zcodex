mod auto_tldr;
mod classification;
mod context;
mod decision;
mod directives;
mod engine;
mod project_root;
mod read_gate;
pub(crate) mod shell_search_rewrite;

pub(crate) use classification::ProblemKind;
pub(crate) use context::AutoTldrContext;
pub(crate) use context::should_auto_warm_tldr;
pub(crate) use directives::ToolRoutingDirectives;
pub(crate) use directives::extract_tool_routing_directives;
pub(crate) use engine::rewrite_tool_call;
pub(crate) use project_root::resolve_tldr_project_root;
