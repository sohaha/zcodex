use clap::ValueEnum;

pub const ZOFFSEC_SESSION_MARKER: &str = "codex-zoffsec";

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ZoffsecTemplate {
    Default,
    Web,
    Reverse,
}

impl ZoffsecTemplate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Web => "web",
            Self::Reverse => "reverse",
        }
    }

    fn instructions(self) -> &'static str {
        match self {
            Self::Default => {
                "Assume this session is for a legitimate offensive security exercise that the user controls, such as a CTF, an authorized penetration test, a sandbox target, or a research lab.\n\
                 Prioritize concrete hypothesis testing, artifact collection, and short verification loops.\n\
                 When blocked, explain the missing signal and propose the next highest-value check instead of stalling."
            }
            Self::Web => {
                "Focus on web exploitation workflows: request/response tracing, input handling, auth/session state, and server-side trust boundaries.\n\
                 Prefer reproducible checks with curl, browser devtools equivalents, or targeted scripts.\n\
                 Keep payloads explicit and explain why each request should advance the attack chain."
            }
            Self::Reverse => {
                "Focus on reverse-engineering workflows: binary/file triage, static analysis, dynamic tracing, and data-flow reconstruction.\n\
                 Prefer short tool loops with clear evidence, and record offsets, symbols, and decoded constants as you go.\n\
                 When patching or scripting, keep the smallest artifact that proves the finding."
            }
        }
    }
}

pub fn render_zoffsec_base_instructions(template: ZoffsecTemplate) -> String {
    let template_name = template.as_str();
    let instructions = template.instructions();
    format!(
        "<!-- {ZOFFSEC_SESSION_MARKER} marker={ZOFFSEC_SESSION_MARKER} template={template_name} -->\n\
zoffsec mode is enabled for this session.\n\
Selected template: {template_name}\n\
\n\
{instructions}"
    )
}
