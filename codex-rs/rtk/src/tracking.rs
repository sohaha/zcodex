use std::ffi::OsString;
use std::time::Instant;

/// 为运行期命令包装器保留的最小时序统计适配层。
///
/// Codex 将 RTK 作为轻量命令过滤层嵌入，因此不会包含上游的分析、
/// 持久化或遥测功能。
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
        timer.track_passthrough("git tag", "rtk 回退：git tag");
    }
}
