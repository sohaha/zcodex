use crate::tools::rewrite::classification::ToolRoutingIntent;
use crate::tools::rewrite::classification::apply_tool_routing_updates;
use crate::tools::rewrite::classification::classify_tool_routing_intent;
use codex_protocol::user_input::UserInput;

pub(crate) type ToolRoutingDirectives = ToolRoutingIntent;

pub(crate) fn extract_tool_routing_directives(input: &[UserInput]) -> ToolRoutingDirectives {
    classify_tool_routing_intent(input)
}

pub(crate) fn merge_tool_routing_directives(
    directives: ToolRoutingDirectives,
    input: &[UserInput],
) -> ToolRoutingDirectives {
    apply_tool_routing_updates(directives, input)
}
