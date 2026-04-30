//! Mission Skill 文件管理。
//!
//! 负责从已安装的 mission skill 目录加载 skill 文件。

use crate::MissionResult;
use crate::error::MissionError;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Mission Skill 工作目录名称（相对于 CODEX_HOME/skills）。
pub const MISSION_SKILLS_DIR_NAME: &str = "mission";

/// Mission Skill 工作目录名称（相对于 workspace）。
pub const MISSION_SKILLS_WORK_DIR: &str = ".mission_skills";

/// Mission Skill 文件列表。
pub const MISSION_SKILL_FILES: &[&str] = &[
    "mission-planning.md",
    "define-mission-skills.md",
    "mission-worker-base.md",
    "scrutiny-validator.md",
    "user-testing-validator.md",
];

/// Mission Skill 管理器。
///
/// 负责从已安装的 mission skill 目录加载 skill 文件，并在运行时复制到工作目录。
#[derive(Debug, Clone)]
pub struct MissionSkillManager {
    /// CODEX_HOME 中的 mission skills 目录。
    source_dir: PathBuf,
    /// 工作目录。
    workspace: PathBuf,
}

impl MissionSkillManager {
    /// 创建新的 Mission Skill 管理器。
    ///
    /// # 参数
    ///
    /// * `codex_home` - CODEX_HOME 目录
    /// * `workspace` - 工作区目录
    pub fn new(codex_home: impl AsRef<Path>, workspace: impl AsRef<Path>) -> Self {
        let source_dir = codex_home
            .as_ref()
            .join("skills")
            .join(MISSION_SKILLS_DIR_NAME);

        Self {
            source_dir,
            workspace: workspace.as_ref().to_path_buf(),
        }
    }

    /// 获取 skill 工作目录路径。
    pub fn work_dir(&self) -> PathBuf {
        self.workspace.join(MISSION_SKILLS_WORK_DIR)
    }

    /// 安装 Mission skill 文件到工作目录。
    ///
    /// 如果工作目录已存在且内容相同，则跳过安装。
    pub fn install(&self) -> MissionResult<()> {
        let work_dir = self.work_dir();

        // 检查源目录是否存在
        if !self.source_dir.exists() {
            return Err(MissionError::SkillNotFound {
                name: MISSION_SKILLS_DIR_NAME.to_string(),
                path: self.source_dir.clone(),
            });
        }

        // 创建工作目录
        fs::create_dir_all(&work_dir).map_err(|source| MissionError::CreateSkillDir {
            path: work_dir.clone(),
            source,
        })?;

        // 复制 skill 文件
        for skill_file in MISSION_SKILL_FILES {
            self.install_skill_file(skill_file)?;
        }

        Ok(())
    }

    /// 安装单个 skill 文件。
    fn install_skill_file(&self, skill_file: &str) -> MissionResult<()> {
        let work_dir = self.work_dir();
        let source_path = self.source_dir.join(skill_file);
        let dest_path = work_dir.join(skill_file);

        // 检查源文件是否存在
        if !source_path.exists() {
            return Err(MissionError::SkillTemplateNotFound {
                name: skill_file.to_string(),
            });
        }

        // 检查文件是否已存在且内容相同
        if dest_path.exists() {
            let existing_content =
                fs::read_to_string(&dest_path).map_err(|source| MissionError::ReadSkillFile {
                    path: dest_path.clone(),
                    source,
                })?;
            let source_content =
                fs::read_to_string(&source_path).map_err(|source| MissionError::ReadSkillFile {
                    path: source_path.clone(),
                    source,
                })?;

            if existing_content == source_content {
                return Ok(());
            }
        }

        // 复制文件
        fs::copy(&source_path, &dest_path).map_err(|source| MissionError::WriteSkillFile {
            path: dest_path,
            source,
        })?;

        Ok(())
    }

    /// 获取 skill 文件路径。
    pub fn skill_path(&self, skill_name: &str) -> MissionResult<PathBuf> {
        let skill_file = format!("{skill_name}.md");
        let path = self.work_dir().join(&skill_file);

        if !path.exists() {
            return Err(MissionError::SkillNotFound {
                name: skill_file,
                path: self.work_dir(),
            });
        }

        Ok(path)
    }

    /// 读取 skill 文件内容。
    pub fn read_skill(&self, skill_name: &str) -> MissionResult<String> {
        let path = self.skill_path(skill_name)?;

        fs::read_to_string(&path).map_err(|source| MissionError::ReadSkillFile { path, source })
    }

    /// 列出所有可用的 skill 文件。
    pub fn list_skills(&self) -> MissionResult<Vec<String>> {
        let work_dir = self.work_dir();

        if !work_dir.exists() {
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();
        let entries = fs::read_dir(&work_dir).map_err(|source| MissionError::ReadSkillDir {
            path: work_dir,
            source,
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    skills.push(name.to_string());
                }
            }
        }

        skills.sort();
        Ok(skills)
    }

    /// 清理 skill 工作目录。
    pub fn cleanup(&self) -> MissionResult<()> {
        let work_dir = self.work_dir();

        if work_dir.exists() {
            fs::remove_dir_all(&work_dir).map_err(|source| MissionError::CleanupSkillDir {
                path: work_dir,
                source,
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn install_creates_skill_files() {
        let codex_home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();

        // 创建源目录和 skill 文件
        let source_dir = codex_home.path().join("skills").join("mission");
        fs::create_dir_all(&source_dir).unwrap();
        for skill_file in MISSION_SKILL_FILES {
            fs::write(
                source_dir.join(skill_file),
                format!("Content of {skill_file}"),
            )
            .unwrap();
        }

        let manager = MissionSkillManager::new(codex_home.path(), workspace.path());
        manager.install().unwrap();

        for skill_file in MISSION_SKILL_FILES {
            let path = manager.work_dir().join(skill_file);
            assert!(path.exists(), "{skill_file} 应该存在");
        }
    }

    #[test]
    fn read_skill_returns_content() {
        let codex_home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();

        // 创建源目录和 skill 文件
        let source_dir = codex_home.path().join("skills").join("mission");
        fs::create_dir_all(&source_dir).unwrap();
        for skill_file in MISSION_SKILL_FILES {
            let content = if *skill_file == "mission-planning.md" {
                "Mission Planning Content"
            } else {
                ""
            };
            fs::write(source_dir.join(skill_file), content).unwrap();
        }

        let manager = MissionSkillManager::new(codex_home.path(), workspace.path());
        manager.install().unwrap();

        let content = manager.read_skill("mission-planning").unwrap();
        assert_eq!(content, "Mission Planning Content");
    }

    #[test]
    fn list_skills_returns_all_skills() {
        let codex_home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();

        // 创建源目录和 skill 文件
        let source_dir = codex_home.path().join("skills").join("mission");
        fs::create_dir_all(&source_dir).unwrap();
        for skill_file in MISSION_SKILL_FILES {
            fs::write(
                source_dir.join(skill_file),
                format!("Content of {skill_file}"),
            )
            .unwrap();
        }

        let manager = MissionSkillManager::new(codex_home.path(), workspace.path());
        manager.install().unwrap();

        let skills = manager.list_skills().unwrap();
        assert!(skills.contains(&"mission-planning".to_string()));
        assert!(skills.contains(&"define-mission-skills".to_string()));
        assert!(skills.contains(&"mission-worker-base".to_string()));
        assert!(skills.contains(&"scrutiny-validator".to_string()));
        assert!(skills.contains(&"user-testing-validator".to_string()));
    }

    #[test]
    fn cleanup_removes_skill_directory() {
        let codex_home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();

        // 创建源目录和 skill 文件
        let source_dir = codex_home.path().join("skills").join("mission");
        fs::create_dir_all(&source_dir).unwrap();
        for skill_file in MISSION_SKILL_FILES {
            fs::write(source_dir.join(skill_file), "Content").unwrap();
        }

        let manager = MissionSkillManager::new(codex_home.path(), workspace.path());
        manager.install().unwrap();
        assert!(manager.work_dir().exists());

        manager.cleanup().unwrap();
        assert!(!manager.work_dir().exists());
    }

    #[test]
    fn skill_path_returns_correct_path() {
        let codex_home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();

        // 创建源目录和 skill 文件
        let source_dir = codex_home.path().join("skills").join("mission");
        fs::create_dir_all(&source_dir).unwrap();
        for skill_file in MISSION_SKILL_FILES {
            fs::write(source_dir.join(skill_file), "Content").unwrap();
        }

        let manager = MissionSkillManager::new(codex_home.path(), workspace.path());
        manager.install().unwrap();

        let path = manager.skill_path("mission-planning").unwrap();
        assert!(path.ends_with(".mission_skills/mission-planning.md"));
    }

    #[test]
    fn skill_path_returns_error_for_missing_skill() {
        let codex_home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();

        // 创建源目录和 skill 文件
        let source_dir = codex_home.path().join("skills").join("mission");
        fs::create_dir_all(&source_dir).unwrap();
        for skill_file in MISSION_SKILL_FILES {
            fs::write(source_dir.join(skill_file), "Content").unwrap();
        }

        let manager = MissionSkillManager::new(codex_home.path(), workspace.path());
        manager.install().unwrap();

        let result = manager.skill_path("non-existent");
        assert!(matches!(result, Err(MissionError::SkillNotFound { .. })));
    }
}
