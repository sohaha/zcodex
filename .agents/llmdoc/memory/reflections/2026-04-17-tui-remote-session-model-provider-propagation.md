# 反思：TUI 远端 app-server session 也必须显式转发 `model_provider`

## 背景
- 当前改动落在 `codex-rs/tui/src/app_server_session.rs`。
- TUI 通过 app-server 的 `thread/start`、`thread/resume`、`thread/fork` 三条线程生命周期 RPC 启动或接管会话。
- 之前的实现把 `Remote` 模式下的 `model_provider` 和 `cwd` 一起省略，导致远端 session 只能依赖 server 端默认 provider。

## 根因
- `ThreadParamsMode` 里把“哪些字段在远端模式下不能本地决定”混成了一个粗粒度判断。
- 实际上两类字段语义不同：
  - `cwd`：远端 session 只有在客户端显式提供 `remote_cwd_override` 时才能安全下发，否则应省略。
  - `model_provider`：这是客户端配置的一部分，embedded / remote 两种模式都应显式透传，不能因为 `cwd` 省略就一起丢掉。

## 修正方式
- 把 `ThreadParamsMode::model_provider_from_config()` 收敛为 embedded / remote 均返回 `Some(config.model_provider_id.clone())`。
- 保持 `thread_cwd_from_config()` 的既有分流：embedded 传本地 `config.cwd`，remote 仅在有 override 时传值。
- 同步更新 `thread_start_params_from_config()`、`thread_resume_params_from_config()`、`thread_fork_params_from_config()` 的远端测试断言，确保三条 RPC 一致。

## 教训
1. 远端 session 的参数裁剪不能按“整组本地上下文”粗暴处理，要逐字段区分“客户端配置”与“远端运行时状态”。
2. 只要字段属于线程身份或模型选择 contract，就应让 start/resume/fork 三条生命周期入口保持同一组断言，避免其中一条悄悄回退到 server 默认值。
3. `cwd` 与 `model_provider` 看起来都像“会话上下文”，但一个是远端环境事实，一个是客户端策略选择；这类字段不能共用同一个省略规则。

## 复用建议
- 以后在 `app_server_session.rs` 增加远端线程参数时，先判断字段属于哪一类：
  - 远端环境事实：默认省略，只在远端 override 明确存在时下发。
  - 客户端策略/配置：embedded 与 remote 都显式下发。
- 若修改了 start/resume/fork 任一入口的参数构造，测试必须覆盖三者，避免 contract 漏洞只出现在其中一个生命周期动作上。
