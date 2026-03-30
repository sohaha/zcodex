use crate::tools::rewrite::classification::ToolRoutingIntent;
use crate::tools::rewrite::classification::classify_tool_routing_intent;
use codex_protocol::user_input::UserInput;

pub(crate) type ToolRoutingDirectives = ToolRoutingIntent;

pub(crate) fn extract_tool_routing_directives(input: &[UserInput]) -> ToolRoutingDirectives {
    classify_tool_routing_intent(input)
}
