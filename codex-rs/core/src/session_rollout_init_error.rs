use std::io::ErrorKind;
use std::path::Path;

use crate::rollout::SESSIONS_SUBDIR;
use codex_protocol::error::CodexErr;

pub(crate) fn map_session_init_error(err: &anyhow::Error, codex_home: &Path) -> CodexErr {
    if let Some(mapped) = err
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .find_map(|io_err| map_rollout_io_error(io_err, codex_home))
    {
        return mapped;
    }

    CodexErr::Fatal(format!("会话初始化失败：{err:#}"))
}

fn map_rollout_io_error(io_err: &std::io::Error, codex_home: &Path) -> Option<CodexErr> {
    let sessions_dir = codex_home.join(SESSIONS_SUBDIR);
    let hint = match io_err.kind() {
        ErrorKind::PermissionDenied => format!(
            "Codex 无法访问 {} 中的会话文件（权限被拒绝）。如果这些会话是使用 sudo 创建的，请修复所有权：sudo chown -R $(whoami) {}",
            sessions_dir.display(),
            codex_home.display()
        ),
        ErrorKind::NotFound => format!(
            "在 {} 未找到会话存储目录。请创建该目录，或改用其他 Codex home。",
            sessions_dir.display()
        ),
        ErrorKind::AlreadyExists => format!(
            "会话存储路径 {} 被现有文件占用。请移除或重命名它，以便 Codex 创建会话。",
            sessions_dir.display()
        ),
        ErrorKind::InvalidData | ErrorKind::InvalidInput => format!(
            "{} 下的会话数据似乎已损坏或不可读。可以尝试清理 sessions 目录（这会删除已保存线程）。",
            sessions_dir.display()
        ),
        ErrorKind::IsADirectory | ErrorKind::NotADirectory => format!(
            "会话存储路径 {} 的类型异常。请确保它是 Codex 可用于保存会话文件的目录。",
            sessions_dir.display()
        ),
        _ => return None,
    };

    Some(CodexErr::Fatal(format!(
        "{hint} (underlying error: {io_err})"
    )))
}
