# 2026-04-18 TUI 启动黑屏占位

## 背景
- 用户反馈 `codex-cli` 首次启动时，要过一段时间才显示 TUI。
- 目标不是先做大规模启动重构，而是先确认黑屏来自哪里，并用最小改动改善首次启动体感。

## 观察
- `codex-rs/cli/src/main.rs` 默认无子命令时直接进入 `codex_tui::run_main(...)`。
- `codex-rs/tui/src/lib.rs` 的 `run_main` 在 `tui::init()` 之后、事件循环之前，还会串行执行多段初始化：
  - app-server 启动
  - 登录状态读取
  - onboarding / trust / resume 分支判定
  - 后续 `App::run` 内部的 `bootstrap` 和线程启动
- 因为首屏绘制发生得太晚，用户体感是“黑屏卡住”，即使进程实际上在工作。

## 本次处理
- 在 `tui::init()` 和 `Tui::new(...)` 之后立刻绘制一个轻量启动占位屏。
- 复用现有 `shimmer` 视觉语言，显示“Codex 正在启动 / 正在初始化会话与界面，请稍候。”。
- 这样即使 app-server 或启动期远程调用仍然串行，用户也能马上看到界面反馈，而不是等待黑屏结束。

## 结论
- 启动体感问题不一定都需要先改成异步化；如果真实瓶颈前已经有可用终端上下文，优先把首帧提前往往是风险最低、收益最快的修复。
- 后续若继续优化真实耗时，应优先审查：
  - `start_app_server(...)`
  - `get_login_status(...)`
  - `App::run(...)` 开头的 `bootstrap(...)`
- 这些步骤是否能拆成“首屏后异步进行”需要额外验证启动流程不变量，尤其是 onboarding、resume/fork、模型迁移提示与初始线程配置顺序。

## 额外发现
- 当前本地 `codex-tui` 测试链路存在与本任务无关的编译错误，来自现有工作树而非本次改动：
  - `tui/src/app.rs` 含异常标识符 `AI 助手Message` / `CollabAI 助手Tool`
  - `tui/src/chatwidget/tests/popups_and_settings.rs` 存在重复测试定义
- 这类仓库既有错误会阻断局部验证；排查启动问题时应先区分“本次改动引入的问题”和“工作树既有阻塞”。
