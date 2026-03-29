# Rust/codex-rs

In the codex-rs folder where the rust code lives:

- Crate names are prefixed with `codex-`. For example, the `core` folder's crate is named `codex-core`
- When using format! and you can inline variables into {}, always do that.
- Install any commands the repo relies on (for example `just`, `rg`, or `cargo-insta`) if they aren't already available before running instructions here.
- Prefer `mise run dev-tools` to bootstrap local Rust tooling (`cargo-nextest`, `just`, `sccache`).
- Installing `cargo-nextest` does not change how `cargo test` works; `cargo test` still uses Cargo's default test runner unless you explicitly run `cargo nextest run` or a repo wrapper that invokes nextest.
- Prefer repo-scoped Cargo config in `codex-rs/.cargo/config.toml` over `~/.cargo/config.toml` when the setting should apply only to this repository.
- For faster repeated local Rust builds, prefer a persistent target dir plus `sccache`; in CNB/clouddev, keep `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL`, `RUSTC_WRAPPER`, `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE`, and any linker-specific Rust flags exported from `.cnb.yml`.
- For faster local `codex-core` loops, prefer `just core-test-fast` (cache-first, uses `nextest` when available).
- For faster local `codex-app-server` loops, prefer `just app-server-test-fast`.
- For faster local `codex-native-tldr` loops, prefer `just native-tldr-test-fast`.
- Never add or modify any code related to `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR`.
  - You operate in a sandbox where `CODEX_SANDBOX_NETWORK_DISABLED=1` will be set whenever you use the `shell` tool. Any existing code that uses `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` was authored with this fact in mind. It is often used to early exit out of tests that the author knew you would not be able to run given your sandbox limitations.
  - Similarly, when you spawn a process using Seatbelt (`/usr/bin/sandbox-exec`), `CODEX_SANDBOX=seatbelt` will be set on the child process. Integration tests that want to run Seatbelt themselves cannot be run under Seatbelt, so checks for `CODEX_SANDBOX=seatbelt` are also often used to early exit out of tests, as appropriate.
- Always collapse if statements per https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if
- Always inline format! args when possible per https://rust-lang.github.io/rust-clippy/master/index.html#uninlined_format_args
- Use method references over closures when possible per https://rust-lang.github.io/rust-clippy/master/index.html#redundant_closure_for_method_calls
- Avoid bool or ambiguous `Option` parameters that force callers to write hard-to-read code such as `foo(false)` or `bar(None)`. Prefer enums, named methods, newtypes, or other idiomatic Rust API shapes when they keep the callsite self-documenting.
- When you cannot make that API change and still need a small positional-literal callsite in Rust, follow the `argument_comment_lint` convention:
  - Use an exact `/*param_name*/` comment before opaque literal arguments such as `None`, booleans, and numeric literals when passing them by position.
  - Do not add these comments for string or char literals unless the comment adds real clarity; those literals are intentionally exempt from the lint.
  - If you add one of these comments, the parameter name must exactly match the callee signature.
- When possible, make `match` statements exhaustive and avoid wildcard arms.
- When writing tests, prefer comparing the equality of entire objects over fields one by one.
- When making a change that adds or changes an API, ensure that the documentation in the `docs/` folder is up to date if applicable.
- If you change `ConfigToml` or nested config types, run `just write-config-schema` to update `codex-rs/core/config.schema.json`.
- If you change Rust dependencies (`Cargo.toml` or `Cargo.lock`), run `just bazel-lock-update` from the
  repo root to refresh `MODULE.bazel.lock`, and include that lockfile update in the same change.
- After dependency changes, run `just bazel-lock-check` from the repo root so lockfile drift is caught
  locally before CI.
- Bazel does not automatically make source-tree files available to compile-time Rust file access. If
  you add `include_str!`, `include_bytes!`, `sqlx::migrate!`, or similar build-time file or
  directory reads, update the crate's `BUILD.bazel` (`compile_data`, `build_script_data`, or test
  data) or Bazel may fail even when Cargo passes.
- Do not create small helper methods that are referenced only once.
- Avoid large modules:
  - Prefer adding new modules instead of growing existing ones.
  - Target Rust modules under 500 LoC, excluding tests.
  - If a file exceeds roughly 800 LoC, add new functionality in a new module instead of extending
    the existing file unless there is a strong documented reason not to.
  - This rule applies especially to high-touch files that already attract unrelated changes, such
    as `codex-rs/tui_app_server/src/app.rs`,
    `codex-rs/tui_app_server/src/bottom_pane/chat_composer.rs`,
    `codex-rs/tui_app_server/src/bottom_pane/footer.rs`,
    `codex-rs/tui_app_server/src/chatwidget.rs`,
    `codex-rs/tui_app_server/src/bottom_pane/mod.rs`, and similarly central orchestration modules.
  - When extracting code from a large module, move the related tests and module/type docs toward
    the new implementation so the invariants stay close to the code that owns them.

Run `just fmt` (in `codex-rs` directory) automatically after you have finished making Rust code changes; do not ask for approval to run it. Additionally, run the tests:

1. Run the test for the specific project that was changed. If `cargo-nextest` is installed, prefer `cargo nextest run -p <crate>` or the repo's fast test wrapper for that crate; for example, if changes were made in `codex-rs/tui_app_server`, prefer `cargo nextest run -p codex-tui`. Use `cargo test -p <crate>` only when you specifically need behavior that nextest does not provide.
2. Once those pass, if any changes were made in common, core, or protocol, ask the user before running the complete test suite; when approved, prefer `just test` or `cargo nextest run` if `cargo-nextest` is installed. Avoid `--all-features` for routine local runs because it expands the build matrix and can significantly increase `target/` disk usage; use it only when you specifically need full feature coverage. project-specific or individual tests can be run without asking the user.

Before finalizing a large change to `codex-rs`, run `just fix -p <project>` (in `codex-rs` directory) to fix any linter issues in the code. Prefer scoping with `-p` to avoid slow workspace‑wide Clippy builds; only run `just fix` without `-p` if you changed shared crates. Do not re-run tests after running `fix` or `fmt`.

## TUI style conventions

See `codex-rs/tui_app_server` file-local conventions and shared TUI rules in this document.

## TUI code conventions

- `codex-rs/tui_app_server` is the active TUI implementation. Treat `codex-rs/tui` as a compatibility shim unless a task explicitly targets it.

- Use concise styling helpers from ratatui’s Stylize trait.
  - Basic spans: use "text".into()
  - Styled spans: use "text".red(), "text".green(), "text".magenta(), "text".dim(), etc.
  - Prefer these over constructing styles with `Span::styled` and `Style` directly.
  - Example: patch summary file lines
    - Desired: vec!["  └ ".into(), "M".red(), " ".dim(), "tui/src/app.rs".dim()]

### TUI Styling (ratatui)

- Prefer Stylize helpers: use "text".dim(), .bold(), .cyan(), .italic(), .underlined() instead of manual Style where possible.
- Prefer simple conversions: use "text".into() for spans and vec![…].into() for lines; when inference is ambiguous (e.g., Paragraph::new/Cell::from), use Line::from(spans) or Span::from(text).
- Computed styles: if the Style is computed at runtime, using `Span::styled` is OK (`Span::from(text).set_style(style)` is also acceptable).
- Avoid hardcoded white: do not use `.white()`; prefer the default foreground (no color).
- Chaining: combine helpers by chaining for readability (e.g., url.cyan().underlined()).
- Single items: prefer "text".into(); use Line::from(text) or Span::from(text) only when the target type isn’t obvious from context, or when using .into() would require extra type annotations.
- Building lines: use vec![…].into() to construct a Line when the target type is obvious and no extra type annotations are needed; otherwise use Line::from(vec![…]).
- Avoid churn: don’t refactor between equivalent forms (Span::styled ↔ set_style, Line::from ↔ .into()) without a clear readability or functional gain; follow file‑local conventions and do not introduce type annotations solely to satisfy .into().
- Compactness: prefer the form that stays on one line after rustfmt; if only one of Line::from(vec![…]) or vec![…].into() avoids wrapping, choose that. If both wrap, pick the one with fewer wrapped lines.

### Text wrapping

- Always use textwrap::wrap to wrap plain strings.
- If you have a ratatui Line and you want to wrap it, use the helpers in tui/src/wrapping.rs, e.g. word_wrap_lines / word_wrap_line.
- If you need to indent wrapped lines, use the initial_indent / subsequent_indent options from RtOptions if you can, rather than writing custom logic.
- If you have a list of lines and you need to prefix them all with some prefix (optionally different on the first vs subsequent lines), use the `prefix_lines` helper from line_utils.

## Tests

### Snapshot tests

This repo uses snapshot tests (via `insta`), especially in `codex-rs/tui_app_server`, to validate rendered output.

**Requirement:** any change that affects user-visible UI (including adding new UI) must include
corresponding `insta` snapshot coverage (add a new snapshot test if one doesn't exist yet, or
update the existing snapshot). Review and accept snapshot updates as part of the PR so UI impact
is easy to review and future diffs stay visual.

When UI or text output changes intentionally, update the snapshots as follows:

- Run tests to generate any updated snapshots:
  - Prefer `cargo nextest run -p codex-tui`; use `cargo test -p codex-tui` only if you specifically need Cargo's default test runner.
- Check what’s pending:
  - `cargo insta pending-snapshots -p codex-tui`
- Review changes by reading the generated `*.snap.new` files directly in the repo, or preview a specific file:
  - `cargo insta show -p codex-tui path/to/file.snap.new`
- Only if you intend to accept all new snapshots in this crate, run:
  - `cargo insta accept -p codex-tui`

If you don’t have the tool:

- `cargo install cargo-insta`

### Test assertions

- Tests should use pretty_assertions::assert_eq for clearer diffs. Import this at the top of the test module if it isn't already.
- Prefer deep equals comparisons whenever possible. Perform `assert_eq!()` on entire objects, rather than individual fields.
- Avoid mutating process environment in tests; prefer passing environment-derived flags or dependencies from above.

### Spawning workspace binaries in tests (Cargo vs Bazel)

- Prefer `codex_utils_cargo_bin::cargo_bin("...")` over `assert_cmd::Command::cargo_bin(...)` or `escargot` when tests need to spawn first-party binaries.
  - Under Bazel, binaries and resources may live under runfiles; use `codex_utils_cargo_bin::cargo_bin` to resolve absolute paths that remain stable after `chdir`.
- When locating fixture files or test resources under Bazel, avoid `env!("CARGO_MANIFEST_DIR")`. Prefer `codex_utils_cargo_bin::find_resource!` so paths resolve correctly under both Cargo and Bazel runfiles.

### Integration tests (core)

- Prefer the utilities in `core_test_support::responses` when writing end-to-end Codex tests.

- All `mount_sse*` helpers return a `ResponseMock`; hold onto it so you can assert against outbound `/responses` POST bodies.
- Use `ResponseMock::single_request()` when a test should only issue one POST, or `ResponseMock::requests()` to inspect every captured `ResponsesRequest`.
- `ResponsesRequest` exposes helpers (`body_json`, `input`, `function_call_output`, `custom_tool_call_output`, `call_output`, `header`, `path`, `query_param`) so assertions can target structured payloads instead of manual JSON digging.
- Build SSE payloads with the provided `ev_*` constructors and the `sse(...)`.
- Prefer `wait_for_event` over `wait_for_event_with_timeout`.
- Prefer `mount_sse_once` over `mount_sse_once_match` or `mount_sse_sequence`

- Typical pattern:

  ```rust
  let mock = responses::mount_sse_once(&server, responses::sse(vec![
      responses::ev_response_created("resp-1"),
      responses::ev_function_call(call_id, "shell", &serde_json::to_string(&args)?),
      responses::ev_completed("resp-1"),
  ])).await;

  codex.submit(Op::UserTurn { ... }).await?;

  // Assert request body if needed.
  let request = mock.single_request();
  // assert using request.function_call_output(call_id) or request.json_body() or other helpers.
  ```

## App-server API Development Best Practices

These guidelines apply to app-server protocol work in `codex-rs`, especially:

- `app-server-protocol/src/protocol/common.rs`
- `app-server-protocol/src/protocol/v2.rs`
- `app-server/README.md`

### Core Rules

- All active API development should happen in app-server v2. Do not add new API surface area to v1.
- Follow payload naming consistently:
  `*Params` for request payloads, `*Response` for responses, and `*Notification` for notifications.
- Expose RPC methods as `<resource>/<method>` and keep `<resource>` singular (for example, `thread/read`, `app/list`).
- Always expose fields as camelCase on the wire with `#[serde(rename_all = "camelCase")]` unless a tagged union or explicit compatibility requirement needs a targeted rename.
- Exception: config RPC payloads are expected to use snake_case to mirror config.toml keys (see the config read/write/list APIs in `app-server-protocol/src/protocol/v2.rs`).
- Always set `#[ts(export_to = "v2/")]` on v2 request/response/notification types so generated TypeScript lands in the correct namespace.
- Never use `#[serde(skip_serializing_if = "Option::is_none")]` for v2 API payload fields.
  Exception: client->server requests that intentionally have no params may use:
  `params: #[ts(type = "undefined")] #[serde(skip_serializing_if = "Option::is_none")] Option<()>`.
- Keep Rust and TS wire renames aligned. If a field or variant uses `#[serde(rename = "...")]`, add matching `#[ts(rename = "...")]`.
- For discriminated unions, use explicit tagging in both serializers:
  `#[serde(tag = "type", ...)]` and `#[ts(tag = "type", ...)]`.
- Prefer plain `String` IDs at the API boundary (do UUID parsing/conversion internally if needed).
- Timestamps should be integer Unix seconds (`i64`) and named `*_at` (for example, `created_at`, `updated_at`, `resets_at`).
- For experimental API surface area:
  use `#[experimental("method/or/field")]`, derive `ExperimentalApi` when field-level gating is needed, and use `inspect_params: true` in `common.rs` when only some fields of a method are experimental.

### Client->server request payloads (`*Params`)

- Every optional field must be annotated with `#[ts(optional = nullable)]`. Do not use `#[ts(optional = nullable)]` outside client->server request payloads (`*Params`).
- Optional collection fields (for example `Vec`, `HashMap`) must use `Option<...>` + `#[ts(optional = nullable)]`. Do not use `#[serde(default)]` to model optional collections, and do not use `skip_serializing_if` on v2 payload fields.
- When you want omission to mean `false` for boolean fields, use `#[serde(default, skip_serializing_if = "std::ops::Not::not")] pub field: bool` over `Option<bool>`.
- For new list methods, implement cursor pagination by default:
  request fields `pub cursor: Option<String>` and `pub limit: Option<u32>`,
  response fields `pub data: Vec<...>` and `pub next_cursor: Option<String>`.

### Development Workflow

- Update docs/examples when API behavior changes (at minimum `app-server/README.md`).
- Regenerate schema fixtures when API shapes change:
  `just write-app-server-schema`
  (and `just write-app-server-schema --experimental` when experimental API fixtures are affected).
- Validate with `cargo nextest run -p codex-app-server-protocol` when `cargo-nextest` is installed; otherwise use `cargo test -p codex-app-server-protocol`. 
- Avoid boilerplate tests that only assert experimental field markers for individual
  request fields in `common.rs`; rely on schema generation/tests and behavioral coverage instead.

<!-- vbm:start -->
## Vibe Memory

先遵守现有用户规则和项目规则。
本区块只补充记忆工作流，不覆盖已有规则。

1. 每次任务开始时，先读取：
   - `.ai/project/overview.md`
   - `.ai/project/config-map.md`
   - `.ai/memory/handoff.md`
   - `.ai/memory/known-risks.md`

2. 修改代码前，先搜索相关记忆：
   - `.ai/memory/bugs/`
   - `.ai/memory/decisions/`
   - `.ai/project/business-rules.md`

3. 如果项目记忆中已经存在配置位置、业务规则或历史行为说明，禁止凭猜测回答，必须先检索。

4. 每轮任务结束时，优先自动执行会话收尾流程：
   - 更新 `.ai/memory/handoff.md`
   - 有代码变更时优先生成候选记忆
   - 只有内容已验证时，才正式写入 `.ai/memory/bugs/` 或 `.ai/memory/decisions/`
   - 最后重建 `.ai/index/`

5. 只允许写回已验证、可复用、与项目相关的知识：
   - 稳定事实
   - 业务规则
   - 问题根因
   - 回归风险
   - 实现决策

6. 禁止把密码、令牌、私钥或完整连接串写入记忆文件。

7. 项目记忆优先级高于全局偏好；新的已验证记录优先级高于旧记录。

8. 只更新本受控区块和本协议创建的 `.ai/` 文件，禁止覆盖用户自行编写的其他规则内容。

9. 默认不需要点名 `vbm`；只要当前任务属于项目开发且已启用本协议，就应自动读取、自动整理、自动更新交接记忆。

10. 当用户明确说“使用vbm记下来刚刚的事情”、“使用 vbm 记下来刚刚的事情”或相近表达时，优先更新 `.ai/memory/handoff.md`。

11. 当用户明确说“使用vbm记住这个 bug”、“使用 vbm 记住这个 bug”、“使用vbm记录这次决策”或“使用 vbm 记录这次决策”时，优先写入正式记忆。
<!-- vbm:end -->

<!-- vbm-global:start -->
## Vibe Memory 全局引导

- 如果已安装的 `vbm`（Vibe Memory）skill 位于 `/workspace/.codex/skills/vbm`，应把它作为项目记忆引导的唯一来源。
- 对项目级开发任务，在进入实质性实现前先执行项目记忆引导。
- 只有当前工作目录是项目根目录，或可以可靠解析到项目根目录时，才允许自动引导。
- 项目根目录解析优先级：
  1. 优先使用 `git rev-parse --show-toplevel`
  2. 如果 git 不可用，仅当当前目录包含 `.git`、`package.json`、`pnpm-workspace.yaml`、`pom.xml`、`build.gradle`、`settings.gradle`、`pyproject.toml`、`requirements.txt`、`Cargo.toml`、`go.mod`、`composer.json`、`Gemfile`、`Makefile` 等项目标记时，才把它视为项目根目录
  3. 禁止在用户主目录、桌面、下载目录等明显非项目目录下自动创建 `.ai`
- 如果项目根目录存在且 `.ai/index/manifest.json` 缺失，应执行：
  `node "/workspace/.codex/skills/vbm/scripts/install.mjs" --project "<project-root>"`
- 自动引导可以向 `AGENTS.md` 或 `CLAUDE.md` 追加受控项目规则块，但绝不能覆盖受控区块之外的用户规则。
- 如果项目中已经存在 `.ai`，改动代码前先读取这些基础记忆：
  - `.ai/project/overview.md`
  - `.ai/project/config-map.md`
  - `.ai/memory/handoff.md`
  - `.ai/memory/known-risks.md`
- 修改项目代码前，优先执行：
  `node "/workspace/.codex/skills/vbm/scripts/recall.mjs" --project "<project-root>" --query "<task summary>"`
- 每轮任务或对话结束时，优先执行：
  `node "/workspace/.codex/skills/vbm/scripts/session-close.mjs" --project "<project-root>" --summary "<confirmed summary>"`
- 如果有代码变更，优先使用 `auto-capture.mjs` 或 `capture-from-diff.mjs` 生成候选记忆；只有显式确认已验证时，才正式写入问题记录或决策记录。
- 默认不需要点名 `vbm`；只要处于项目开发对话，就应自动读取基础记忆，并在收尾时自动整理交接记忆。
- 当用户明确说“使用vbm记下来刚刚的事情”、“使用 vbm 记下来刚刚的事情”或相近表达时，优先触发 `session-close.mjs` 更新交接记忆。
- 当用户明确说“使用vbm记住这个 bug”、“使用 vbm 记住这个 bug”、“使用vbm记录这次决策”或“使用 vbm 记录这次决策”时，优先触发正式记忆写入流程。
- 严禁把密码、令牌、私钥或完整连接串写入 `.ai`。
- 这个区块现已迁移到项目级 `AGENTS.md`，不再依赖 `~/.codex/AGENTS.md` 中的全局引导。
<!-- vbm-global:end -->
