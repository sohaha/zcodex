# 多配置文件合并加载

> 不适用项写 `无`，不要留空。
> 只写已确认事实；若信息不清楚或需要假设，先回到对话澄清，不要把未确认内容写进计划。

## 背景
- **当前状态**：目前仅从 `${CODEX_HOME}/config.toml`（即 `~/.codex/config.toml`）加载用户配置，社区分叉版本的专属配置（如 zmemory 偏好、定制行为开关）只能写在同一个文件中，容易与官方配置冲突或被官方覆盖。
- **触发原因**：社区分叉版本需要独立的配置文件 `~/.codex/zconfig.toml` 存放分叉特有配置，与官方 `config.toml` 共存并优先覆盖。
- **预期影响**：
  - `~/.codex/config.toml` 继续作为官方配置底层
  - `~/.codex/zconfig.toml` 作为社区专属配置层，优先级更高
  - 两个文件内容合并后生效，重复字段以 `zconfig.toml` 为准

## 目标
- **目标结果**：启动时同时读取 `config.toml` 和 `zconfig.toml`，按正确优先级合并；对用户透明，无需修改现有 `config.toml` 结构。
- **完成定义（DoD）**：
  1. `app-server-protocol` 中的 `ConfigLayerSource` 增加了 `ZConfig` 变体
  2. `config/src/state.rs` 中的 `ConfigLayerEntry::config_folder()` 正确处理新变体
  3. `core/src/config_loader/mod.rs` 加载流程增加了对 `zconfig.toml` 的读取
  4. `core/src/config_loader/mod.rs` 中 `load_config_layers_state` 返回的层栈包含两个独立的用户层
  5. 合并逻辑正确：`config.toml` → `zconfig.toml`（后者覆盖前者）
  6. 现有功能（无 `zconfig.toml` 时）行为不变
  7. 单元测试覆盖新增加载路径
  8. `just fmt` 和 `cargo nextest run -p codex-core` 通过
- **非目标**：
  - 不修改 `config.toml` 的 schema（仅 `codex-config` crate 范围）
  - 不做 `zconfig.toml` 的写入支持（只读）
  - 不引入新的配置文件路径（固定为 `${CODEX_HOME}/zconfig.toml`）

## 范围
- **范围内**：
  - `app-server-protocol/src/protocol/v2.rs`：`ConfigLayerSource` 枚举增加 `ZConfig` 变体
  - `config/src/state.rs`：`ConfigLayerEntry::config_folder()` 处理 `ZConfig` 变体
  - `core/src/config_loader/mod.rs`：增加 `zconfig.toml` 读取与层构建逻辑
  - `config/src/lib.rs`：增加 `ZCONFIG_TOML_FILE` 常量
  - 现有配置合并逻辑（`config/src/merge.rs`）已支持任意层合并，无需修改
- **范围外**：
  - 不修改 TUI、CLI 等上层模块
  - 不修改 `ConfigToml` 结构的字段定义
  - 不修改配置写入路径（`mcp_edit.rs`、`marketplace_edit.rs` 等）

## 影响
- **受影响模块**：`codex-core` 配置加载子系统
- **受影响接口/命令**：无公开 API 变化；内部层栈新增一个层
- **受影响数据/模式**：新增配置层 `ConfigLayerSource::ZConfig`
- **受影响用户界面/行为**：无 UI 变化；仅配置加载行为

## 约束与依赖
- **约束**：
  - `ConfigLayerSource` 在 `app-server-protocol` 中定义，属于跨 crate 共享类型，需同步修改
  - `zconfig.toml` 不存在时不能报错，应静默跳过
  - 合并优先级：`config.toml`（低）< `zconfig.toml`（高）< `SessionFlags`
  - 需要与现有 `ConfigLayerSource::User` 的 precedence 体系兼容
- **外部依赖**：无

## 实施策略
- **总体方案**：
  1. 在 `app-server-protocol/src/protocol/v2.rs` 的 `ConfigLayerSource` 中增加 `ZConfig { file: AbsolutePathBuf }` 变体，设置 precedence 高于 `User` 但低于 `SessionFlags`
  2. 在 `config/src/state.rs` 的 `ConfigLayerEntry::config_folder()` 中增加对新变体的处理
  3. 在 `config/src/lib.rs` 中增加 `pub const ZCONFIG_TOML_FILE: &str = "zconfig.toml";`
  4. 在 `core/src/config_loader/mod.rs` 的 `load_config_layers_state` 中，在 `User` 层之后、CLI 层之前，增加对 `zconfig.toml` 的可选加载

- **关键决策**：
  - 为什么用独立层而非直接修改 `User` 层内容：
    - 保持配置来源可追溯（`origins()` API 仍能区分来源）
    - 与现有层架构一致，改动最小
    - 未来如需暴露 `zconfig.toml` 特有字段，不会破坏现有 schema
  - 为什么 `ZConfig` precedence 高于 `User`：
    - 用户明确要求"zconfig.toml 优先级更高"
    - 与 `User` 是同一目录下的两个文件，逻辑上同一类配置，ZConfig 是 User 的扩展/覆盖

- **明确不采用的方案**：
  - 不采用：在 `User` 层加载后对 `TomlValue` 做运行时替换（会丢失来源追踪）
  - 不采用：直接修改 `ConfigToml::merge`（已有 `merge_toml_values` 支持层级别合并）

## 阶段拆分

### Phase 1：协议层扩展
- **目标**：在 `ConfigLayerSource` 中新增 `ZConfig` 变体并设置优先级
- **交付物**：
  - `app-server-protocol/src/protocol/v2.rs`：`ConfigLayerSource::ZConfig` 变体
- **完成条件**：`cargo build -p codex-app-server-protocol` 通过
- **依赖**：无

### Phase 2：配置层基础设施
- **目标**：在 `codex-config` crate 中增加对 `ZConfig` 变体的支持
- **交付物**：
  - `config/src/lib.rs`：`ZCONFIG_TOML_FILE` 常量
  - `config/src/state.rs`：`ConfigLayerEntry::config_folder()` 对 `ZConfig` 的处理
- **完成条件**：`cargo build -p codex-config` 通过
- **依赖**：Phase 1

### Phase 3：配置加载逻辑
- **目标**：在 `load_config_layers_state` 中增加 `zconfig.toml` 的读取
- **交付物**：
  - `core/src/config_loader/mod.rs`：增加 `ZConfig` 层的加载逻辑（在 User 层之后、SessionFlags 之前）
- **完成条件**：`cargo build -p codex-core` 通过
- **依赖**：Phase 2

### Phase 4：测试与验证
- **目标**：确保新增逻辑正确，且不影响现有行为
- **交付物**：
  - 单元测试：验证 `zconfig.toml` 存在时被正确加载并覆盖 `config.toml`
  - 单元测试：验证 `zconfig.toml` 不存在时行为不变
- **完成条件**：`cargo nextest run -p codex-core` 通过
- **依赖**：Phase 3

## 测试与验证
- **核心验证**：
  - `cargo build -p codex-app-server-protocol && cargo build -p codex-config && cargo build -p codex-core`
- **必过检查**：
  - `just fmt`（Rust 格式化）
  - `cargo nextest run -p codex-core`（核心配置层测试）
- **回归验证**：
  - 现有 `config_loader` 测试套件全部通过
- **手动检查**：
  - 确认 `origins()` API 对 `ZConfig` 层返回正确的 `ConfigLayerSource`
  - 确认 `config_folder()` 对 `ZConfig` 层返回正确的 `.codex/` 目录

## 风险与缓解
- **关键风险**：
  - `ConfigLayerSource` 是跨 crate 共享的枚举，修改会影响所有使用方
  - precedence 顺序错误会导致配置覆盖方向反转
- **触发信号**：
  - CI 中 `codex-app-server-protocol`、`codex-config`、`codex-core` 任意编译失败
  - 现有测试回归
- **缓解措施**：
  - Phase 1 先只修改枚举，确保 build 通过后再进入后续阶段
  - precedence 值：`User`=20，`ZConfig`=21（`ZConfig` > `User`），`SessionFlags`=30
  - 每个 Phase 后运行 `cargo build` 验证编译

## 参考
- `app-server-protocol/src/protocol/v2.rs:485`：`ConfigLayerSource` 定义与 precedence 实现
- `config/src/state.rs`：`ConfigLayerEntry::config_folder()` 实现
- `core/src/config_loader/mod.rs:120-280`：`load_config_layers_state` 完整加载流程
- `config/src/merge.rs`：`merge_toml_values` 工具函数
