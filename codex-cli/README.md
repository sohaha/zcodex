<h1 align="center">OpenAI Codex CLI</h1>
<p align="center">运行在终端里的轻量编程代理</p>

<p align="center"><code>npm i -g @sohaha/zcodex</code></p>

> [!IMPORTANT]
> 这是 Codex CLI _旧版_ TypeScript 实现的文档，已被 _Rust_ 实现取代。详情见 [Codex 仓库根目录 README](https://github.com/sohaha/zcodex/blob/main/README.md)。

![Codex 演示 GIF：使用 codex "explain this codebase to me"](../.github/demo.gif)

---

<details>
<summary><strong>目录</strong></summary>

<!-- Begin ToC -->

- [实验性技术免责声明](#实验性技术免责声明)
- [快速开始](#快速开始)
- [为什么是 Codex？](#为什么是-codex)
- [安全模型与权限](#安全模型与权限)
  - [平台沙箱细节](#平台沙箱细节)
- [系统要求](#系统要求)
- [CLI 参考](#cli-参考)
- [记忆与项目文档](#记忆与项目文档)
- [非交互 / CI 模式](#非交互--ci-模式)
- [Tracing / 详细日志](#tracing--详细日志)
- [实用示例](#实用示例)
- [安装](#安装)
- [配置指南](#配置指南)
  - [基础配置参数](#基础配置参数)
  - [自定义 AI 提供方配置](#自定义-ai-提供方配置)
  - [历史记录配置](#历史记录配置)
  - [配置示例](#配置示例)
  - [完整配置示例](#完整配置示例)
  - [自定义指令](#自定义指令)
  - [环境变量设置](#环境变量设置)
- [常见问题](#常见问题)
- [零数据保留 (ZDR) 使用](#零数据保留-zdr-使用)
- [Codex 开源基金](#codex-开源基金)
- [贡献](#贡献)
  - [开发流程](#开发流程)
  - [Husky Git Hooks](#husky-git-hooks)
  - [调试](#调试)
  - [高影响改动指南](#高影响改动指南)
  - [提交 Pull Request](#提交-pull-request)
  - [评审流程](#评审流程)
  - [社区价值观](#社区价值观)
  - [获取帮助](#获取帮助)
  - [贡献者许可协议（CLA）](#贡献者许可协议cla)
    - [快速修复](#快速修复)
  - [发布 `codex`](#发布-codex)
  - [其他构建方式](#其他构建方式)
    - [Nix flake 开发](#nix-flake-开发)
- [安全与负责任 AI](#安全与负责任-ai)
- [许可证](#许可证)

<!-- End ToC -->

</details>

---

## 实验性技术免责声明

Codex CLI 是一个仍在积极开发的实验性项目，尚未稳定，可能包含 Bug、不完整功能或发生破坏性变更。我们与社区一起公开构建，欢迎：

- 问题报告
- 功能需求
- Pull Request（PR）
- 友好互动

欢迎提交 issue 或 PR 帮助改进（贡献方式见下文）。

## 快速开始

全局安装：

```shell
npm install -g @sohaha/zcodex
```

然后把 OpenAI API Key 写入环境变量：

```shell
export OPENAI_API_KEY="your-api-key-here"
```

> **注意：**该命令只对当前终端会话生效。你可以把 `export` 行写入 shell 配置文件（例如 `~/.zshrc`），但我们建议仅在会话内设置。**提示：**也可以把 API Key 放到项目根目录的 `.env` 文件：
>
> ```env
> OPENAI_API_KEY=your-api-key-here
> ```
>
> CLI 会自动从 `.env` 读取变量（通过 `dotenv/config`）。

<details>
<summary><strong>使用 <code>--provider</code> 切换其他模型/提供方</strong></summary>

> Codex 也支持 OpenAI Chat Completions API 兼容的其他提供方。你可以在配置文件里设置，或通过 `--provider` 指定。可选值包括：
>
> - openai (default)
> - openrouter
> - azure
> - gemini
> - ollama
> - mistral
> - deepseek
> - xai
> - groq
> - arceeai
> - 任何兼容 OpenAI API 的其他提供方
>
> 如果使用 OpenAI 以外的提供方，需要在配置或环境变量中设置对应的 API Key：
>
> ```shell
> export <provider>_API_KEY="your-api-key-here"
> ```
>
> 如果使用未列出的提供方，还需要设置它的 base URL：
>
> ```shell
> export <provider>_BASE_URL="https://your-provider-api-base-url"
> ```

</details>
<br />

交互式运行：

```shell
codex
```

或者传入提示词运行（可选 `Full Auto` 模式）：

```shell
codex "explain this codebase to me"
```

```shell
codex --approval-mode full-auto "create the fanciest todo-list app"
```

就这样——Codex 会生成文件、在沙箱内运行、安装缺失依赖，并展示实时结果。确认改动后会写入你的工作目录。

---

## 为什么是 Codex？

Codex CLI 面向那些**常驻终端**的开发者，提供 ChatGPT 级别的推理能力，同时能真正运行代码、操作文件并在版本控制下迭代。简而言之，它是能理解并执行仓库的“对话驱动开发”。

- **零配置**：提供 OpenAI API Key 即可使用
- **全自动审批且安全**：默认禁用网络并限制目录沙箱
- **多模态**：可输入截图或图示来实现功能 ✨

并且**完全开源**，你可以看到并参与它的演进。

---

## 安全模型与权限

Codex 通过 `--approval-mode`（或交互式引导）让你决定代理的自主程度与自动审批策略：

| 模式                      | 代理可在无需询问下执行                                                                              | 仍需审批                                                                                        |
| ------------------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| **Suggest** <br>(默认)    | <li>读取仓库内任意文件                                                                              | <li>**所有**写入/补丁<li> **任意** shell 命令（读取文件除外）                                     |
| **Auto Edit**             | <li>读取并应用 apply-patch 写入                                                                     | <li>**所有** shell 命令                                                                          |
| **Full Auto**             | <li>读写文件<li>执行 shell 命令（禁用网络，写入限制在工作目录）                                      | -                                                                                               |

在 **Full Auto** 中，所有命令都会在**禁用网络**的环境下运行，并限制在当前工作目录（及临时文件）内以实现纵深防护。如果在未被 Git 跟踪的目录中启动 **auto-edit** 或 **full-auto**，Codex 会提示警告/确认，确保你有安全网。

即将支持：在具备更多安全保障后，允许将特定命令加入白名单并在联网下自动执行。

### 平台沙箱细节

Codex 的加固机制取决于操作系统：

- **macOS 12+**：命令通过 **Apple Seatbelt**（`sandbox-exec`）封装执行。

  - 除少量可写目录（`$PWD`、`$TMPDIR`、`~/.codex` 等）外均为只读。
  - 默认**完全阻断**出站网络，子进程即使尝试 `curl` 也会失败。

- **Linux**：默认无沙箱。建议使用 Docker 沙箱：Codex 会在**最小化容器镜像**中启动，并把你的仓库以同路径 _读写_ 挂载。自定义 `iptables`/`ipset` 防火墙脚本会阻断除 OpenAI API 之外的所有出站流量，在不需要宿主机 root 权限的情况下实现可复现运行。可使用 [`run_in_container.sh`](../codex-cli/scripts/run_in_container.sh) 进行配置。

---

## 系统要求

| 要求                       | 说明                                                            |
| --------------------------- | --------------------------------------------------------------- |
| 操作系统                   | macOS 12+、Ubuntu 20.04+/Debian 10+，或通过 **WSL2** 的 Windows 11 |
| Node.js                    | **16 或更高**（推荐 Node 20 LTS）                               |
| Git（可选，推荐）          | 2.23+（用于内置 PR 辅助）                                       |
| 内存                        | 至少 4 GB（推荐 8 GB）                                          |

> 不要运行 `sudo npm install -g`，请先修复 npm 权限。

---

## CLI 参考

| 命令                                 | 作用                                | 示例                                 |
| ------------------------------------ | ----------------------------------- | ------------------------------------ |
| `codex`                              | 交互式 REPL                          | `codex`                              |
| `codex "..."`                        | 交互式 REPL 的初始提示词             | `codex "fix lint errors"`            |
| `codex -q "..."`                     | 非交互“静默模式”                     | `codex -q --json "explain utils.ts"` |
| `codex completion <bash\|zsh\|fish>` | 输出 shell 补全脚本                  | `codex completion bash`              |

常用参数：`--model/-m`、`--approval-mode/-a`、`--quiet/-q`、`--notify`。

---

## 记忆与项目文档

你可以通过 `AGENTS.md` 给 Codex 额外指令与指导。Codex 会按以下位置自上而下合并：

1. `~/.codex/AGENTS.md`：个人全局指引
2. 仓库根目录 `AGENTS.md`：共享项目说明
3. 当前工作目录 `AGENTS.md`：子目录/功能级说明

可通过 `--no-project-doc` 或环境变量 `CODEX_DISABLE_PROJECT_DOC=1` 禁用加载。

---

## 非交互 / CI 模式

在流水线中以无 UI 方式运行 Codex。示例 GitHub Action：

```yaml
- name: Update changelog via Codex
  run: |
    npm install -g @sohaha/zcodex
    export OPENAI_API_KEY="${{ secrets.OPENAI_KEY }}"
    codex -a auto-edit --quiet "update CHANGELOG for next release"
```

设置 `CODEX_QUIET_MODE=1` 可屏蔽交互 UI 输出。

## Tracing / 详细日志

设置环境变量 `DEBUG=true` 可打印完整的 API 请求与响应详情：

```shell
DEBUG=true codex
```

---

## 实用示例

下面是一些可直接复制粘贴的示例，把引号中的文字替换为你的任务即可。更多提示与用法见 [prompting guide](https://github.com/sohaha/zcodex/blob/main/codex-cli/examples/prompting_guide.md)。

| ✨  | 你输入的内容                                                                  | 执行结果                                           |
| --- | ----------------------------------------------------------------------------- | -------------------------------------------------- |
| 1   | `codex "Refactor the Dashboard component to React Hooks"`                      | 重写类组件、运行 `npm test` 并展示 diff。            |
| 2   | `codex "Generate SQL migrations for adding a users table"`                     | 推断 ORM、生成迁移文件，并在沙箱 DB 中执行。         |
| 3   | `codex "Write unit tests for utils/date.ts"`                                   | 生成测试、执行并迭代直到通过。                      |
| 4   | `codex "Bulk-rename *.jpeg -> *.jpg with git mv"`                              | 安全重命名文件并更新引用/用法。                      |
| 5   | `codex "Explain what this regex does: ^(?=.*[A-Z]).{8,}$"`                     | 输出逐步解释。                                     |
| 6   | `codex "Carefully review this repo, and propose 3 high impact well-scoped PRs"`| 提供 3 个高影响且范围清晰的 PR 建议。               |
| 7   | `codex "Look for vulnerabilities and create a security review report"`         | 发现并解释安全问题。                               |

---

## 安装

<details open>
<summary><strong>通过 npm 安装（推荐）</strong></summary>

```bash
npm install -g @sohaha/zcodex
# 或
yarn global add @sohaha/zcodex
# 或
bun install -g @sohaha/zcodex
# 或
pnpm add -g @sohaha/zcodex
```

</details>

<details>
<summary><strong>从源码构建</strong></summary>

```bash
# 克隆仓库并进入 CLI 包目录
git clone https://github.com/sohaha/zcodex.git
cd zcodex/codex-cli

# 启用 corepack
corepack enable

# 安装依赖并构建
pnpm install
pnpm build

# 仅 Linux：下载预编译沙箱二进制（需要 gh 和 zstd）。
./scripts/install_native_deps.sh

# 查看使用说明与参数
node ./dist/cli.js --help

# 直接运行本地构建的 CLI
node ./dist/cli.js

# 或者全局链接，方便调用
pnpm link
```

Rust 原生构建以及 Ubuntu 交叉构建 macOS arm64 / Windows amd64/arm64 的说明见 [`docs/install.md`](../docs/install.md)。

</details>

---

## 配置指南

Codex 配置文件放在 `~/.codex/`，支持 YAML 和 JSON。

### 基础配置参数

| 参数                | 类型    | 默认值     | 说明                              | 可选值                                                                                         |
| ------------------- | ------- | ---------- | -------------------------------- | ---------------------------------------------------------------------------------------------- |
| `model`             | string  | `o4-mini`  | 使用的模型                        | 任意支持 OpenAI API 的模型名                                                                   |
| `approvalMode`      | string  | `suggest`  | 代理权限模式                      | `suggest`（仅建议）<br>`auto-edit`（自动修改）<br>`full-auto`（全自动）                          |
| `fullAutoErrorMode` | string  | `ask-user` | full-auto 出错处理方式            | `ask-user`（询问用户）<br>`ignore-and-continue`（忽略并继续）                                   |
| `notify`            | boolean | `true`     | 桌面通知                          | `true`/`false`                                                                                 |

### 自定义 AI 提供方配置

在 `providers` 中可以配置多个 AI 提供方。每个提供方需要以下参数：

| 参数      | 类型   | 说明                                    | 示例                          |
| --------- | ------ | --------------------------------------- | ----------------------------- |
| `name`    | string | 提供方显示名                            | `"OpenAI"`                    |
| `baseURL` | string | API 服务 URL                            | `"https://api.openai.com/v1"` |
| `envKey`  | string | API Key 的环境变量名                    | `"OPENAI_API_KEY"`            |

### 历史记录配置

在 `history` 中配置对话历史：

| 参数                | 类型    | 说明                                                   | 示例值        |
| ------------------- | ------- | ------------------------------------------------------ | ------------- |
| `maxSize`           | number  | 最大历史条目数                                         | `1000`        |
| `saveHistory`       | boolean | 是否保存历史                                           | `true`        |
| `sensitivePatterns` | array   | 历史中过滤敏感信息的模式                               | `[]`          |

### 配置示例

1. YAML 格式（保存为 `~/.codex/config.yaml`）：

```yaml
model: o4-mini
approvalMode: suggest
fullAutoErrorMode: ask-user
notify: true
```

2. JSON 格式（保存为 `~/.codex/config.json`）：

```json
{
  "model": "o4-mini",
  "approvalMode": "suggest",
  "fullAutoErrorMode": "ask-user",
  "notify": true
}
```

### 完整配置示例

下面是包含多个自定义提供方的 `config.json` 示例：

```json
{
  "model": "o4-mini",
  "provider": "openai",
  "providers": {
    "openai": {
      "name": "OpenAI",
      "baseURL": "https://api.openai.com/v1",
      "envKey": "OPENAI_API_KEY"
    },
    "azure": {
      "name": "AzureOpenAI",
      "baseURL": "https://YOUR_PROJECT_NAME.openai.azure.com/openai",
      "envKey": "AZURE_OPENAI_API_KEY"
    },
    "openrouter": {
      "name": "OpenRouter",
      "baseURL": "https://openrouter.ai/api/v1",
      "envKey": "OPENROUTER_API_KEY"
    },
    "gemini": {
      "name": "Gemini",
      "baseURL": "https://generativelanguage.googleapis.com/v1beta/openai",
      "envKey": "GEMINI_API_KEY"
    },
    "ollama": {
      "name": "Ollama",
      "baseURL": "http://localhost:11434/v1",
      "envKey": "OLLAMA_API_KEY"
    },
    "mistral": {
      "name": "Mistral",
      "baseURL": "https://api.mistral.ai/v1",
      "envKey": "MISTRAL_API_KEY"
    },
    "deepseek": {
      "name": "DeepSeek",
      "baseURL": "https://api.deepseek.com",
      "envKey": "DEEPSEEK_API_KEY"
    },
    "xai": {
      "name": "xAI",
      "baseURL": "https://api.x.ai/v1",
      "envKey": "XAI_API_KEY"
    },
    "groq": {
      "name": "Groq",
      "baseURL": "https://api.groq.com/openai/v1",
      "envKey": "GROQ_API_KEY"
    },
    "arceeai": {
      "name": "ArceeAI",
      "baseURL": "https://conductor.arcee.ai/v1",
      "envKey": "ARCEEAI_API_KEY"
    }
  },
  "history": {
    "maxSize": 1000,
    "saveHistory": true,
    "sensitivePatterns": []
  }
}
```

### 自定义指令

你可以创建 `~/.codex/AGENTS.md` 为代理定义自定义指引：

```markdown
- Always respond with emojis
- Only use git commands when explicitly requested
```

### 环境变量设置

对每个 AI 提供方，都需要在环境变量中设置对应的 API Key，例如：

```bash
# OpenAI
export OPENAI_API_KEY="your-api-key-here"

# Azure OpenAI
export AZURE_OPENAI_API_KEY="your-azure-api-key-here"
export AZURE_OPENAI_API_VERSION="2025-04-01-preview"（可选）

# OpenRouter
export OPENROUTER_API_KEY="your-openrouter-key-here"

# 其他提供方同理
```

---

## 常见问题

<details>
<summary>OpenAI 在 2021 年发布过 Codex 模型，这和这里有关吗？</summary>

2021 年 OpenAI 发布了 Codex 模型，用于从自然语言生成代码。该模型已在 2023 年 3 月弃用，与这里的 CLI 工具不同。

</details>

<details>
<summary>支持哪些模型？</summary>

[Responses API](https://platform.openai.com/docs/api-reference/responses) 提供的模型均可使用。默认是 `o4-mini`，可通过 `--model gpt-4.1` 或在配置里设置 `model: gpt-4.1` 覆盖。

</details>
<details>
<summary>为什么 <code>o3</code> 或 <code>o4-mini</code> 对我不可用？</summary>

可能需要先完成 [API 账号验证](https://help.openai.com/en/articles/10910291-api-organization-verification)，才能启用流式响应并获取思维链摘要。如果仍有问题，请告诉我们。

</details>

<details>
<summary>如何阻止 Codex 修改文件？</summary>

Codex 会在沙箱中执行模型生成的命令。如果某条命令或文件改动不合适，直接输入 **n** 拒绝，或给模型反馈即可。

</details>
<details>
<summary>支持 Windows 吗？</summary>

不直接支持。需要 [Windows Subsystem for Linux (WSL2)](https://learn.microsoft.com/en-us/windows/wsl/install)。Codex 常规测试环境为 macOS/Linux + Node 20+，也支持 Node 16。

</details>

---

## 零数据保留 (ZDR) 使用

Codex CLI **支持**启用 [Zero Data Retention (ZDR)](https://platform.openai.com/docs/guides/your-data#zero-data-retention) 的 OpenAI 组织。如果已启用 ZDR 但仍遇到如下错误：

```
OpenAI rejected the request. Error details: Status: 400, Code: unsupported_parameter, Type: invalid_request_error, Message: 400 Previous response cannot be used for this organization due to Zero Data Retention.
```

可能需要升级到新版：`npm i -g @sohaha/zcodex@latest`

---

## Codex 开源基金

我们很高兴推出 **100 万美元计划**，支持使用 Codex CLI 与其他 OpenAI 模型的开源项目。

- 最高可获得 **25,000 美元**的 API 额度资助。
- 申请 **滚动审核**。

**感兴趣？[点击申请](https://openai.com/form/codex-open-source-fund/)。**

---

## 贡献

该项目仍在积极开发中，代码可能会发生较大变化。稳定后会更新此说明。

总体而言我们欢迎贡献——无论你是第一次提交 PR，还是资深维护者。同时我们非常重视可靠性与长期可维护性，所以合并标准刻意 **较高**。下文将“高质量”的实践标准讲清楚，尽量让流程透明且友好。

### 开发流程

- 从 `main` 创建主题分支（_topic branch_），例如 `feat/interactive-prompt`。
- 保持改动聚焦，互不相关的修复请拆成多个 PR。
- 开发期间建议使用 `pnpm test:watch` 获得快速反馈。
- 单测使用 **Vitest**，风格用 **ESLint** + **Prettier**，类型检查用 **TypeScript**。
- 推送前运行完整测试/类型/格式检查：

### Husky Git Hooks

本项目使用 [Husky](https://typicode.github.io/husky/) 来强制代码质量检查：

- **Pre-commit hook**：提交前自动运行 lint-staged 进行格式化与检查
- **Pre-push hook**：推送前运行测试和类型检查

这些钩子可维持代码质量，防止推送带失败测试的代码。详情见 [HUSKY.md](./HUSKY.md)。

```bash
pnpm test && pnpm run lint && pnpm run typecheck
```

- 如果你**还未**签署 CLA，请在 PR 中添加如下评论（原文一致）：

  ```text
  I have read the CLA Document and I hereby sign the CLA
  ```

  所有作者签署后，CLA-Assistant 机器人会将 PR 状态标绿。

```bash
# 监听模式（代码变更时重跑测试）
pnpm test:watch

# 仅做类型检查，不生成文件
pnpm typecheck

# 自动修复 lint + prettier 问题
pnpm lint:fix
pnpm format:fix
```

### 调试

在 `codex-cli` 目录中使用可视化调试器的步骤如下：

- 运行 `pnpm run build` 构建 CLI，会在 `dist` 中生成 `cli.js.map`。
- 用 `node --inspect-brk ./dist/cli.js` 运行，程序会等待调试器连接。可选方式：
  - VS Code：命令面板选择 **Debug: Attach to Node Process**，下拉选 `9229` 端口（通常第一个）。
  - Chrome：打开 <chrome://inspect>，找到 **localhost:9229** 并点击 **trace**。

### 高影响改动指南

1. **先从 issue 开始。** 新开或跟进已有讨论，先对方案达成一致再写代码。
2. **补充/更新测试。** 新功能或修复应有测试覆盖：改动前失败、改动后通过。无需 100% 覆盖，但要有意义。
3. **更新文档。** 若影响用户行为，请更新 README、内置帮助（`codex --help`）或示例项目。
4. **提交原子化。** 每个提交都能编译并通过测试，便于评审与回滚。

### 提交 Pull Request

- 填写 PR 模板（或包含类似信息）：**What? Why? How?**
- 本地运行 **全部**检查（`npm test && npm run lint && npm run typecheck`）。本地可捕获的失败会拖慢 CI。
- 确保分支与 `main` 同步并解决冲突。
- 仅在你认为可合并时标记为 **Ready for review**。

### 评审流程

1. 会指派一名维护者作为主审。
2. 可能会要求修改——请不要介意，我们同样重视一致性和长期可维护性。
3. 达成共识后，维护者会 squash-and-merge。

### 社区价值观

- **友善包容。** 尊重他人；我们遵循 [Contributor Covenant](https://www.contributor-covenant.org/)。
- **假定善意。** 文字沟通不易，多一点善意。
- **教学相长。** 发现困惑之处，可开 issue 或 PR 改进。

### 获取帮助

如果你在搭建项目时遇到问题、想要反馈想法，或只是打个招呼，请开 Discussion 或进入相关 issue，我们很乐意帮助。

一起把 Codex CLI 做得更好。**开发愉快！** :rocket:

### 贡献者许可协议（CLA）

所有贡献者**必须**签署 CLA，流程很轻量：

1. 打开你的 PR。
2. 粘贴如下评论（若曾签过可回复 `recheck`）：

   ```text
   I have read the CLA Document and I hereby sign the CLA
   ```

3. CLA-Assistant 机器人会记录签名并将状态检查标记为通过。

无需特殊 Git 命令、邮件附件或提交脚注。

#### 快速修复

| 场景              | 命令                                           |
| ----------------- | ------------------------------------------------ |
| 修改上次提交      | `git commit --amend -s --no-edit && git push -f` |

**DCO 检查**会阻止合并，直到 PR 中每个提交都带有 footer（squash 后只需一个）。

### 发布 `codex`

发布 CLI 新版本前需先打包 npm 包。`codex-cli/scripts/` 中的脚本会完成大部分工作。在 `codex-cli` 目录中运行：

```bash
# 传统 JS 实现，包含 Linux 沙箱用的小型原生二进制。
pnpm stage-release

# 可指定临时目录以复用产物。
RELEASE_DIR=$(mktemp -d)
pnpm stage-release --tmp "$RELEASE_DIR"

# “大包”版本：额外打包 Linux 版 Rust CLI 二进制。终端用户可通过设置 CODEX_RUST=1 启用。
pnpm stage-release --native
```

进入打包产物目录验证功能正常后，在临时目录执行：

```
cd "$RELEASE_DIR"
npm publish
```

基于 GitHub Actions 的发布请使用 `.github/workflows/rust-release.yml`：

- 标签发布：推送 `v1.2.3`；CI 自动从标签推导 Rust 与 npm 版本。
- 手动发布：运行 `workflow_dispatch` 并填写 `release-version`；`release-tag` 可选，默认 `v<release-version>`。
- 安全检查：手动提供 `release-tag` 时，CI 会校验标签内版本与 `release-version` 完全一致，否则快速失败。

手动发布简要步骤：

1. 打开 GitHub Actions，选择 `rust-release`。
2. 点击 `Run workflow`。
3. 填写 `release-version`，如 `1.2.3` 或 `1.2.3-alpha.4`。
4. 可选填写 `release-tag`；未填则使用 `v<release-version>`。
5. 运行工作流并核对 GitHub Release 产物和 npm 发布步骤。

### 其他构建方式

#### Nix flake 开发

前置条件：Nix >= 2.4 且启用 flakes（`~/.config/nix/nix.conf` 中设置 `experimental-features = nix-command flakes`）。

进入 Nix 开发 shell：

```bash
# 根据你要开发的实现选择其一
nix develop .#codex-cli # 进入 codex-cli 专用 shell
nix develop .#codex-rs # 进入 codex-rs 专用 shell
```

该 shell 会包含 Node.js、安装依赖、构建 CLI，并提供 `codex` 命令别名。

直接构建并运行 CLI：

```bash
# 根据你要开发的实现选择其一
nix build .#codex-cli # 构建 codex-cli
nix build .#codex-rs # 构建 codex-rs
./result/bin/codex --help
```

通过 flake app 运行 CLI：

```bash
# 根据你要开发的实现选择其一
nix run .#codex-cli # 运行 codex-cli
nix run .#codex-rs # 运行 codex-rs
```

使用 direnv + flakes

若已安装 direnv，可使用以下 `.envrc` 在进入项目目录时自动进入 Nix shell：

```bash
cd codex-rs
echo "use flake ../flake.nix#codex-cli" >> .envrc && direnv allow
cd codex-cli
echo "use flake ../flake.nix#codex-rs" >> .envrc && direnv allow
```

---

## 安全与负责任 AI

如发现安全漏洞或对模型输出有疑虑，请邮件联系 **security@openai.com**，我们会尽快响应。

---

## 许可证

本仓库采用 [Apache-2.0 License](LICENSE)。
