//! Mission 知识沉淀系统。
//!
//! 负责管理 `.factory/` 目录的知识沉淀，包括服务、库和代理规范。

use crate::MissionResult;
use crate::error::MissionError;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// .factory 目录名称。
pub const FACTORY_DIR_NAME: &str = ".factory";

/// services.yaml 文件名。
pub const SERVICES_FILE_NAME: &str = "services.yaml";

/// library 目录名。
pub const LIBRARY_DIR_NAME: &str = "library";

/// AGENTS.md 文件名。
pub const AGENTS_FILE_NAME: &str = "AGENTS.md";

/// Mission 知识管理器。
///
/// 负责创建和更新 `.factory/` 目录的知识沉淀。
#[derive(Debug, Clone)]
pub struct KnowledgeManager {
    /// 工作区目录。
    workspace: PathBuf,
}

impl KnowledgeManager {
    /// 创建新的知识管理器。
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
        }
    }

    /// 获取 .factory 目录路径。
    pub fn factory_dir(&self) -> PathBuf {
        self.workspace.join(FACTORY_DIR_NAME)
    }

    /// 获取 services.yaml 文件路径。
    pub fn services_path(&self) -> PathBuf {
        self.factory_dir().join(SERVICES_FILE_NAME)
    }

    /// 获取 library 目录路径。
    pub fn library_dir(&self) -> PathBuf {
        self.factory_dir().join(LIBRARY_DIR_NAME)
    }

    /// 获取 AGENTS.md 文件路径。
    pub fn agents_path(&self) -> PathBuf {
        self.factory_dir().join(AGENTS_FILE_NAME)
    }

    /// 初始化 .factory 目录结构。
    ///
    /// 如果目录已存在，则保留现有内容。
    pub fn initialize(&self) -> MissionResult<()> {
        let factory_dir = self.factory_dir();

        // 创建 .factory 目录
        fs::create_dir_all(&factory_dir).map_err(|source| MissionError::CreateFactoryDir {
            path: factory_dir.clone(),
            source,
        })?;

        // 创建 library 目录
        let library_dir = self.library_dir();
        fs::create_dir_all(&library_dir).map_err(|source| MissionError::CreateFactoryDir {
            path: library_dir,
            source,
        })?;

        // 创建或更新 services.yaml
        self.update_services_yaml()?;

        // 创建或更新 AGENTS.md
        self.update_agents_md()?;

        Ok(())
    }

    /// 更新 services.yaml 文件。
    fn update_services_yaml(&self) -> MissionResult<()> {
        let services_path = self.services_path();

        // 如果文件已存在，跳过更新
        if services_path.exists() {
            return Ok(());
        }

        let content = r#"# Mission Services
#
# 此文件记录 Mission 中使用的服务和依赖。

services:
  # 示例服务定义
  # - name: example-service
  #   description: Example service description
  #   type: api|database|queue|cache
  #   url: http://localhost:8080
  #   config:
  #     key: value
"#;

        fs::write(&services_path, content).map_err(|source| MissionError::WriteFactoryFile {
            path: services_path,
            source,
        })?;

        Ok(())
    }

    /// 更新 AGENTS.md 文件。
    fn update_agents_md(&self) -> MissionResult<()> {
        let agents_path = self.agents_path();

        // 如果文件已存在，跳过更新
        if agents_path.exists() {
            return Ok(());
        }

        let content = r#"# Mission Agents

本目录包含 Mission 系统使用的 AI 代理配置和规范。

## Mission Workers

Mission 系统使用多个专门的 Worker 来完成不同的任务：

### 1. Planning Worker
- **职责**: 执行 7 阶段规划流程
- **Skill**: `mission-planning.md`
- **输入**: Mission 目标
- **输出**: 规划阶段记录

### 2. Implementation Worker
- **职责**: 实现具体功能
- **Skill**: `mission-worker-base.md`
- **输入**: 实现规格
- **输出**: Handoff 报告

### 3. Scrutiny Validator
- **职责**: 代码审查验证
- **Skill**: `scrutiny-validator.md`
- **输入**: Handoff 报告
- **输出**: 验证报告

### 4. User Testing Validator
- **职责**: 用户测试验证
- **Skill**: `user-testing-validator.md`
- **输入**: Handoff 报告
- **输出**: 验证报告

## Handoff 格式

所有 Worker 必须使用标准化的 Handoff JSON 格式进行交接：

```json
{
  "worker": "worker-name",
  "timestamp": "ISO-8601 timestamp",
  "salientSummary": "1-2 sentence summary",
  "whatWasImplemented": ["change 1", "change 2"],
  "filesModified": [{"path": "file.rs", "changeSummary": "description"}],
  "filesCreated": [{"path": "new.rs", "purpose": "description"}],
  "verification": {
    "codeReview": {
      "status": "passed|failed|partial|skipped",
      "findings": "summary",
      "issuesFound": 0,
      "issuesFixed": 0
    },
    "userTesting": {
      "status": "passed|failed|partial|skipped",
      "results": "summary",
      "testCasesExecuted": 0,
      "testCasesPassed": 0
    },
    "remainingWork": "description"
  },
  "nextSteps": "guidance for next worker",
  "blockers": ["blocker 1", "blocker 2"]
}
```

## 验证流程

1. **Scrutiny Validation**: 代码质量、安全性、可维护性
2. **User Testing Validation**: 功能正确性、用户体验、集成

## 知识管理

- **services.yaml**: 服务和依赖配置
- **library/**: 可复用代码库和模板
- **AGENTS.md**: 本文件
"#;

        fs::write(&agents_path, content).map_err(|source| MissionError::WriteFactoryFile {
            path: agents_path,
            source,
        })?;

        Ok(())
    }

    /// 记录 Mission 学习到的知识到 library。
    ///
    /// 将 Mission 过程中产生的有用知识、模式、最佳实践等保存到 library。
    pub fn record_knowledge(
        &self,
        category: impl AsRef<str>,
        name: impl AsRef<str>,
        content: impl AsRef<str>,
    ) -> MissionResult<PathBuf> {
        let category_dir = self.library_dir().join(category.as_ref());
        fs::create_dir_all(&category_dir).map_err(|source| MissionError::CreateFactoryDir {
            path: category_dir.clone(),
            source,
        })?;

        // 使用安全的文件名
        let safe_name = name
            .as_ref()
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '-',
            })
            .collect::<String>();

        let file_path = category_dir.join(format!("{}.md", safe_name));

        // 如果文件已存在，添加新内容
        let mut final_content = if file_path.exists() {
            fs::read_to_string(&file_path).map_err(|source| MissionError::ReadFactoryFile {
                path: file_path.clone(),
                source,
            })?
        } else {
            String::new()
        };

        // 添加时间戳分隔符
        if !final_content.is_empty() {
            final_content.push_str("\n\n---\n\n");
        }

        final_content.push_str(&format!("## {}\n\n", chrono::Utc::now().to_rfc3339()));
        final_content.push_str(content.as_ref());

        fs::write(&file_path, final_content).map_err(|source| MissionError::WriteFactoryFile {
            path: file_path.clone(),
            source,
        })?;

        Ok(file_path)
    }

    /// 更新 services.yaml，添加新服务。
    pub fn add_service(&self, service: ServiceDefinition) -> MissionResult<()> {
        let services_path = self.services_path();

        // 读取现有内容
        let existing = if services_path.exists() {
            fs::read_to_string(&services_path).map_err(|source| MissionError::ReadFactoryFile {
                path: services_path.clone(),
                source,
            })?
        } else {
            String::from("services:\n")
        };

        // 追加新服务
        let mut updated = existing;
        updated.push_str(&format!(
            r#"
  - name: {}
  - description: {}
  - type: {}
"#,
            service.name, service.description, service.service_type
        ));

        if let Some(url) = &service.url {
            updated.push_str(&format!("  - url: {}\n", url));
        }

        fs::write(&services_path, updated).map_err(|source| MissionError::WriteFactoryFile {
            path: services_path,
            source,
        })?;

        Ok(())
    }

    /// 列出 library 中的知识类别。
    pub fn list_categories(&self) -> MissionResult<Vec<String>> {
        let library_dir = self.library_dir();

        if !library_dir.exists() {
            return Ok(Vec::new());
        }

        let mut categories = Vec::new();
        let entries =
            fs::read_dir(&library_dir).map_err(|source| MissionError::ReadFactoryDir {
                path: library_dir,
                source,
            })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    categories.push(name.to_string());
                }
            }
        }

        categories.sort();
        Ok(categories)
    }

    /// 列出指定类别中的知识文件。
    pub fn list_knowledge(&self, category: impl AsRef<str>) -> MissionResult<Vec<KnowledgeEntry>> {
        let category_dir = self.library_dir().join(category.as_ref());

        if !category_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let dir_entries =
            fs::read_dir(&category_dir).map_err(|source| MissionError::ReadFactoryDir {
                path: category_dir,
                source,
            })?;

        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let metadata = path.metadata().ok();
                    entries.push(KnowledgeEntry {
                        name: name.to_string(),
                        path,
                        modified: metadata.and_then(|m| m.modified().ok()),
                    });
                }
            }
        }

        entries.sort_by(|a, b| {
            b.modified
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .cmp(&a.modified.unwrap_or(std::time::SystemTime::UNIX_EPOCH))
        });

        Ok(entries)
    }

    /// 读取指定的知识文件。
    pub fn read_knowledge(
        &self,
        category: impl AsRef<str>,
        name: impl AsRef<str>,
    ) -> MissionResult<String> {
        let file_name = format!("{}.md", name.as_ref());
        let path = self.library_dir().join(category.as_ref()).join(&file_name);

        fs::read_to_string(&path).map_err(|source| MissionError::ReadFactoryFile { path, source })
    }
}

/// 服务定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// 服务名称。
    pub name: String,
    /// 服务描述。
    pub description: String,
    /// 服务类型。
    pub service_type: String,
    /// 服务 URL（可选）。
    pub url: Option<String>,
}

/// 知识条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeEntry {
    /// 知识条目名称。
    pub name: String,
    /// 文件路径。
    pub path: PathBuf,
    /// 修改时间。
    pub modified: Option<std::time::SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn initialize_creates_factory_directory() {
        let workspace = TempDir::new().unwrap();
        let manager = KnowledgeManager::new(workspace.path());

        manager.initialize().unwrap();

        assert!(manager.factory_dir().exists());
        assert!(manager.services_path().exists());
        assert!(manager.library_dir().exists());
        assert!(manager.agents_path().exists());
    }

    #[test]
    fn record_knowledge_creates_file() {
        let workspace = TempDir::new().unwrap();
        let manager = KnowledgeManager::new(workspace.path());
        manager.initialize().unwrap();

        let path = manager
            .record_knowledge("patterns", "test-pattern", "Test content")
            .unwrap();

        assert!(path.exists());
        assert!(
            path.to_string_lossy()
                .contains(".factory/library/patterns/test-pattern.md")
        );
    }

    #[test]
    fn list_categories_returns_empty_for_new_factory() {
        let workspace = TempDir::new().unwrap();
        let manager = KnowledgeManager::new(workspace.path());
        manager.initialize().unwrap();

        let categories = manager.list_categories().unwrap();
        assert!(categories.is_empty());
    }

    #[test]
    fn list_knowledge_returns_entries() {
        let workspace = TempDir::new().unwrap();
        let manager = KnowledgeManager::new(workspace.path());
        manager.initialize().unwrap();

        manager
            .record_knowledge("patterns", "pattern-1", "Content 1")
            .unwrap();
        manager
            .record_knowledge("patterns", "pattern-2", "Content 2")
            .unwrap();

        let entries = manager.list_knowledge("patterns").unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn read_knowledge_returns_content() {
        let workspace = TempDir::new().unwrap();
        let manager = KnowledgeManager::new(workspace.path());
        manager.initialize().unwrap();

        manager
            .record_knowledge("patterns", "test", "Test content")
            .unwrap();

        let content = manager.read_knowledge("patterns", "test").unwrap();
        assert!(content.contains("Test content"));
    }

    #[test]
    fn add_service_appends_to_services_yaml() {
        let workspace = TempDir::new().unwrap();
        let manager = KnowledgeManager::new(workspace.path());
        manager.initialize().unwrap();

        let service = ServiceDefinition {
            name: "test-service".to_string(),
            description: "Test service".to_string(),
            service_type: "api".to_string(),
            url: Some("http://localhost:8080".to_string()),
        };

        manager.add_service(service).unwrap();

        let content = fs::read_to_string(manager.services_path()).unwrap();
        assert!(content.contains("test-service"));
    }
}
