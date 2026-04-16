<p align="center"><strong>zcodex</strong> — 社区分支版本的 Codex CLI</p>

<p align="center">这是 OpenAI <a href="https://github.com/openai/codex">Codex CLI</a> 的社区维护分支，由 <a href="https://github.com/sohaha">sohaha</a> 维护。
<p align="center">
  <img src="./.github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>

<p align="center">
如果你想要官方版本，请访问 <a href="https://github.com/openai/codex">openai/codex</a> 仓库。
</br>如果你想要在 IDE 中使用 Codex（VS Code、Cursor、Windsurf），请<a href="https://developers.openai.com/codex/ide">安装 IDE 扩展</a>。
</br>如果你想要桌面应用体验，运行 <code>codex app</code> 或访问 <a href="https://chatgpt.com/codex?app-landing-page=true">Codex App 页面</a>。
</br>如果你在寻找 OpenAI 的<strong>云端代理</strong> <strong>Codex Web</strong>，请前往 <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>。</p>

---

## Quickstart

### Installing and running zcodex

使用你喜欢的包管理器全局安装：

```shell
# Install using npm
npm install -g @sohaha/zcodex
```

```shell
# Install using Homebrew
# 注意：Homebrew cask 安装的是官方版本
# brew install --cask codex
```

然后直接运行 `codex` 即可开始使用。

<details>
<summary>你也可以前往 <a href="https://github.com/sohaha/zcodex/releases/latest">最新 GitHub Release</a> 下载适合你平台的二进制文件。</summary>

每个 GitHub Release 包含许多可执行文件，但实际上你可能需要以下之一：

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `codex-aarch64-unknown-linux-musl.tar.gz`

每个压缩包包含一个名称中包含平台信息的单个条目（例如 `codex-x86_64-unknown-linux-musl`），所以你可能需要在解压后将其重命名为 `codex`。

</details>

### Using Codex with your ChatGPT plan

运行 `codex` 并选择 **使用 ChatGPT 登录**。我们建议登录你的 ChatGPT 账户，作为 Plus、Pro、Business、Edu 或 Enterprise 计划的一部分使用 Codex。[了解更多关于你的 ChatGPT 计划包含的内容](https://help.openai.com/en/articles/11369540-codex-in-chatgpt)。

你也可以使用 API key 来使用 Codex，但这需要[额外的设置](https://developers.openai.com/codex/auth#sign-in-with-an-api-key)。

## Docs

- [**Codex 官方文档**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**spawn_agent 使用文档**](./docs/spawn_agent.md)
- [**Open source fund**](./docs/open-source-fund.md)

本仓库基于 [Apache-2.0 License](LICENSE) 开源协议。
