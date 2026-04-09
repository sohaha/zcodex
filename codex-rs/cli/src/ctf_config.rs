use clap::ValueEnum;

pub const CTF_SESSION_MARKER: &str = "codex-ctf";

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CtfTemplate {
    Default,
    Web,
    Reverse,
}

impl CtfTemplate {
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
                "Assume this session is for a legitimate CTF or lab challenge that the user controls.\n\
                 Prioritize concrete hypothesis testing, artifact collection, and short verification loops.\n\
                 When blocked, explain the missing signal and propose the next highest-value check instead of stalling."
            }
            Self::Web => {
                "Focus on web challenge workflows: request/response tracing, input handling, auth/session state, and server-side trust boundaries.\n\
                 Prefer reproducible checks with curl, browser devtools equivalents, or targeted scripts.\n\
                 Keep payloads explicit and explain why each request should advance the exploit chain."
            }
            Self::Reverse => {
                "Focus on reverse-engineering workflows: binary/file triage, static analysis, dynamic tracing, and data-flow reconstruction.\n\
                 Prefer short tool loops with clear evidence, and record offsets, symbols, and decoded constants as you go.\n\
                 When patching or scripting, keep the smallest artifact that proves the finding."
            }
        }
    }
}

pub fn render_ctf_base_instructions(template: CtfTemplate) -> String {
    let template_name = template.as_str();
    let instructions = template.instructions();
    format!(
        "<!-- {CTF_SESSION_MARKER} marker={CTF_SESSION_MARKER} template={template_name} -->\n\
CTF mode is enabled for this session.\n\
Selected template: {template_name}\n\
\n\
{instructions}"
    )
}
