use std::ffi::OsString;
use std::time::Instant;

/// Minimal timing shim retained for operational command wrappers.
///
/// Codex embeds RTK as a lightweight command-filter layer and intentionally
/// does not ship upstream analytics, persistence, or telemetry features.
pub struct TimedExecution {
    started_at: Instant,
}

impl TimedExecution {
    #[must_use]
    pub fn start() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn track(&self, _original_cmd: &str, _rtk_cmd: &str, _input: &str, _output: &str) {
        let _ = self.started_at.elapsed();
    }

    pub fn track_passthrough(&self, _original_cmd: &str, _rtk_cmd: &str) {
        let _ = self.started_at.elapsed();
    }
}

#[must_use]
pub fn args_display(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_args_for_passthrough_labels() {
        let args = vec![OsString::from("status"), OsString::from("--short")];
        assert_eq!(args_display(&args), "status --short");
    }

    #[test]
    fn tracking_shim_is_noop() {
        let timer = TimedExecution::start();
        timer.track("git status", "rtk git status", "raw", "filtered");
        timer.track_passthrough("git tag", "rtk fallback: git tag");
    }
}
