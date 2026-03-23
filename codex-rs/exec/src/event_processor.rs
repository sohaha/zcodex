use std::path::Path;

use codex_core::config::Config;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::SessionConfiguredEvent;

pub(crate) enum CodexStatus {
    Running,
    InitiateShutdown,
    Shutdown,
}

pub(crate) trait EventProcessor {
    /// Print summary of effective configuration and user prompt.
    fn print_config_summary(
        &mut self,
        config: &Config,
        prompt: &str,
        session_configured: &SessionConfiguredEvent,
    );

    /// Handle a single event emitted by the agent.
    fn process_event(&mut self, event: Event) -> CodexStatus;

    fn print_final_output(&mut self) {}
}

pub(crate) fn handle_last_message(last_agent_message: Option<&str>, output_file: &Path) {
    let message = last_agent_message.unwrap_or_default();
    write_last_message_file(message, Some(output_file));
    if last_agent_message.is_none() {
        eprintln!(
            "警告：没有最后一条 agent 消息；已向 {} 写入空内容",
            output_file.display()
        );
    }
}

fn write_last_message_file(contents: &str, last_message_path: Option<&Path>) {
    if let Some(path) = last_message_path
        && let Err(e) = std::fs::write(path, contents)
    {
        eprintln!("写入最后一条消息文件失败 {path:?}: {e}");
    }
}
