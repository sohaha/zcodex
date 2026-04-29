# Codex Mission System

Codex Mission 系统是一个完整的工程工作流，从规划到验证，帮助你系统地完成复杂任务。

## 概述

Mission 系统提供：

- **7 阶段规划流程**：系统化的任务分解和规划方法
- **Worker 管理**：专门的 AI Worker 完成不同类型的任务
- **双层验证机制**：代码审查 + 用户测试确保质量
- **知识沉淀**：自动积累项目知识和最佳实践

## 快速开始

### 1. 启动 Mission

```bash
codex mission start "实现用户认证功能"
```

这会启动一个新的 Mission 并进入 7 阶段规划流程。

### 2. 规划阶段

Mission 会依次引导你完成 7 个规划阶段：

1. **目标澄清**：明确目标、成功标准和非目标
2. **上下文收集**：收集相关代码、文档和约束
3. **约束确认**：确认安全、兼容性等约束条件
4. **方案设计**：确定模块边界和数据流
5. **执行计划**：拆解任务顺序和验收标准
6. **Worker 定义**：定义需要的 Worker 类型
7. **验证策略**：定义代码审查和测试策略

在每个阶段完成后，可以使用：

```bash
codex mission continue --note "阶段完成说明"
```

### 3. 查看 Mission 状态

```bash
codex mission status
```

显示当前 Mission 的状态、阶段和进度。

### 4. 验证 Mission

```bash
codex mission validate
```

运行验证器检查代码质量和功能正确性。

## 命令参考

### `codex mission start <goal>`

启动新的 Mission。

**参数：**
- `goal`：Mission 目标描述

**示例：**
```bash
codex mission start "添加数据库迁移功能"
```

### `codex mission status`

显示当前 Mission 状态。

**输出信息：**
- Mission 状态（planning/executing/completed/blocked）
- 当前目标
- 当前阶段（如果在规划中）
- 状态文件路径

### `codex mission continue [--note <说明>]`

推进当前 Mission 到下一个阶段。

**选项：**
- `--note`：记录当前阶段的确认说明

**示例：**
```bash
codex mission continue
codex mission continue --note "上下文收集完成，识别了主要模块"
```

### `codex mission validate [OPTIONS]`

运行验证器并报告结果。

**选项：**
- `--handoff <路径>`：指定 Handoff 文件路径（默认使用最新的）
- `--validator <类型>`：验证器类型（all/scrutiny/user-testing，默认 all）
- `--strict`：严格模式（任何问题都导致失败）
- `--output <格式>`：输出格式（markdown/json，默认 markdown）

**示例：**
```bash
codex mission validate
codex mission validate --validator scrutiny --strict
codex mission validate --output json > report.json
```

## 工作流程

### 完整工作流程

```
1. codex mission start "目标"
   ↓
2. 完成 7 个规划阶段
   ↓
3. Worker 执行任务
   ↓
4. 生成 Handoff
   ↓
5. codex mission validate
   ↓
6. Mission 完成
```

### 规划阶段

每个规划阶段都有：
- **提示**：指导你完成该阶段
- **出口条件**：确认阶段完成的标志

### Worker 执行

Worker 基于 skill 文件执行任务：
- **Planning Worker**：执行规划流程
- **Implementation Worker**：实现功能
- **Validation Worker**：验证质量

### 验证流程

双层验证机制：
1. **Scrutiny Validator**：代码质量、安全性、可维护性
2. **User Testing Validator**：功能正确性、用户体验

## 目录结构

Mission 系统创建以下目录结构：

```
.project/
├── mission_state.json          # Mission 状态
├── worker_sessions/            # Worker session 记录
└── handoffs/                   # Handoff 文件

.factory/
├── services.yaml               # 服务配置
├── library/                    # 知识库
│   ├── patterns/               # 设计模式
│   ├── solutions/              # 解决方案
│   └── best-practices/         # 最佳实践
└── AGENTS.md                   # Agent 配置和规范

.mission_skills/                # Mission skill 文件
├── mission-planning.md
├── mission-worker-base.md
├── scrutiny-validator.md
└── user-testing-validator.md
```

## Handoff 格式

Worker 使用标准化的 Handoff JSON 格式交接：

```json
{
  "worker": "worker-name",
  "timestamp": "2024-04-28T12:34:56Z",
  "salientSummary": "1-2 句话总结",
  "whatWasImplemented": ["变更 1", "变更 2"],
  "filesModified": [
    {
      "path": "src/main.rs",
      "changeSummary": "添加新功能"
    }
  ],
  "filesCreated": [
    {
      "path": "src/test.rs",
      "purpose": "测试文件"
    }
  ],
  "verification": {
    "codeReview": {
      "status": "passed",
      "findings": "审查发现",
      "issuesFound": 2,
      "issuesFixed": 2
    },
    "userTesting": {
      "status": "passed",
      "results": "测试结果",
      "testCasesExecuted": 10,
      "testCasesPassed": 10
    },
    "remainingWork": "剩余工作描述"
  },
  "nextSteps": "下一步建议",
  "blockers": ["阻塞问题 1"]
}
```

## 验证报告

### Scrutiny 验证报告

包含：
- **总体状态**：Passed/Failed/Partial
- **问题列表**：按严重程度分组（Critical/High/Medium/Low）
- **正面发现**：做得好的地方
- **建议**：改进建议

### User Testing 验证报告

包含：
- **总体状态**：Passed/Failed/Partial
- **测试结果**：按分类组织（Smoke/Normal/Edge/Error/Integration）
- **问题列表**：发现的问题和影响
- **建议**：修复建议

## 最佳实践

### 1. 明确目标

启动 Mission 时，目标应该：
- 具体：清楚说明要完成什么
- 可验证：有明确的成功标准
- 范围适当：不要太大或太模糊

**好的目标：**
```
"实现用户登录功能，支持邮箱和密码验证"
```

**不好的目标：**
```
"改进用户体验"
```

### 2. 认真完成每个规划阶段

每个规划阶段都有其目的：
- 不要跳过阶段
- 确保满足出口条件再继续
- 详细记录阶段结果

### 3. 及时生成 Handoff

Worker 完成任务后：
- 立即生成 Handoff
- 确保 salientSummary 清晰简洁
- 列出所有关键变更
- 诚实报告验证状态

### 4. 充分利用验证

运行验证后：
- 仔细阅读验证报告
- 优先处理 Critical 和 High 问题
- 记录学到的经验到 .factory/library

### 5. 积累知识

使用 `KnowledgeManager` 记录：
- 设计模式
- 解决方案
- 最佳实践
- 经验教训

## 故障排除

### Mission 无法启动

**问题**：`codex mission start` 失败

**解决方案**：
1. 检查当前目录是否可写
2. 确保没有其他 Mission 正在运行
3. 查看错误信息了解具体原因

### 验证失败

**问题**：`codex mission validate` 报告失败

**解决方案**：
1. 查看验证报告中的问题列表
2. 优先处理 Critical 问题
3. 修复后重新验证

### Worker 执行失败

**问题**：Worker 无法完成任务

**解决方案**：
1. 检查 Worker session 状态
2. 查看 Handoff 中的 blockers
3. 提供更多上下文或简化任务

## 进阶使用

### 自定义验证器配置

```bash
codex mission validate --strict --validator scrutiny
```

### 导出验证报告

```bash
codex mission validate --output json > report.json
```

### 查看历史 Handoff

```bash
ls -lt .mission/handoffs/
```

### 清理 Mission 状态

```bash
rm -rf .mission
```

**警告**：这会删除所有 Mission 状态，无法恢复。

## 与其他功能的区别

### vs ZTeam

| 特性 | Mission | ZTeam |
|------|---------|-------|
| 用途 | 完整工程工作流 | 本地双 Worker 协作 |
| 规划 | 7 阶段系统化规划 | 简化规划 |
| 验证 | 双层验证机制 | 基础验证 |
| 知识沉淀 | 自动积累 | 手动管理 |
| 适用场景 | 复杂任务、新功能 | 快速协作、简单任务 |

### vs 传统开发流程

| 特性 | Mission | 传统流程 |
|------|---------|----------|
| 规划 | 结构化 7 阶段 | 依赖个人经验 |
| 执行 | AI Worker 辅助 | 手工编写代码 |
| 验证 | 自动双层验证 | 人工代码审查 |
| 知识 | 自动沉淀 | 依赖文档 |

## 参考资源

- [Mission Issue Tracker](.agents/issues/2026-04-28-codex-cli-mission-system.toml)
- [Mission Plan](.agents/plan/2026-04-28-codex-cli-mission-system.md)
- [Skill 文档](codex-rs/skills/src/assets/mission/)
- [.factory/AGENTS.md](.factory/AGENTS.md)

## 反馈和贡献

如果你遇到问题或有建议：

1. 查看现有文档和示例
2. 运行 `codex mission status` 检查状态
3. 查看验证报告了解问题
4. 记录学到的经验到 `.factory/library`

---

**注意**：Mission 系统正在积极开发中，功能和接口可能会变化。
