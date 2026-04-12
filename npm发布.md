# npm 发布流程

本文记录当前 `sohaha/nmtx` 仓库的 npm 发布前置条件、配置项和实际操作步骤。

## 当前发布逻辑

- GitHub Actions workflow: `nmtx/.github/workflows/codex.yml`
- npm scope 已固定为 `@sohaha`
- 发布目标 registry: `https://registry.npmjs.org`
- npm 包名：`@sohaha/zcodex`
- 发布方式：npm trusted publishing（OIDC）
- 不使用 `NPM_TOKEN` / `NODE_AUTH_TOKEN`

## 发布前需要配置的 GitHub Secrets

在 `sohaha/nmtx` 仓库中配置以下 GitHub Actions secrets：

### 必需

- `CODEBAY_RELEASE_WRITE_TOKEN`
- `CNB_TOKEN`

### 仅在构建 macOS 产物时必需

如果 workflow 的 `targets` 包含 macOS 目标，还需要：

- `APPLE_CERTIFICATE_P12`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_NOTARIZATION_KEY_P8`
- `APPLE_NOTARIZATION_KEY_ID`
- `APPLE_NOTARIZATION_ISSUER_ID`

### 可选

- `BARK_URL`

## npm 侧需要做的配置

由于当前走的是 trusted publishing，所以必须在 npm 后台为包配置 trusted publisher。

需要绑定：

- npm package: `@sohaha/zcodex`
- GitHub repository: `sohaha/nmtx`
- GitHub workflow: `codex.yml`

如果这一步没完成：

- GitHub Actions 会执行到 `npm publish`
- 但 npmjs 会拒绝发布

## 不需要配置的变量

当前流程不需要：

- `NPM_TOKEN`
- `NODE_AUTH_TOKEN`
- `NPM_SCOPE`

## 版本规则

以下版本会自动尝试发布 npm：

- 稳定版：`1.2.3`
- alpha 预发布：`1.2.3-alpha.1`

以下版本不会发布 npm，只会继续 GitHub Release 流程：

- 其他不符合上述规则的预发布版本

## 触发发布

可以通过 GitHub Actions 页面手动触发 `codex.yml`，也可以用仓库内置命令：

```bash
mise run trigger-nmtx-codex-ci --version 1.2.3
```

常见参数：

```bash
mise run trigger-nmtx-codex-ci \
  --version 1.2.3 \
  --source-ref web \
  --targets all
```

参数说明：

- `--version`：必填，发布版本号
- `--source-ref`：源码分支或提交，默认 `web`
- `--targets`：目标平台，默认 `all`
- `--release-tag`：可选，默认自动推导为 `v<version>`
- `--release-repo`：默认 `sohaha/zcodex`

## 目标平台与 macOS 证书要求

如果你暂时没配 macOS 签名 secrets，可以只发布非 macOS 平台，例如：

```bash
mise run trigger-nmtx-codex-ci \
  --version 1.2.3 \
  --targets windows,linux
```

如果使用默认的 `all`，则会包含 macOS 目标，此时必须提前配置 macOS 签名相关 secrets。

## 实际发布顺序

workflow 执行顺序如下：

1. 解析版本、源码引用和目标平台
2. 构建各平台二进制
3. 生成并更新 GitHub Release
4. 生成 npm tarball，并上传到 GitHub Release
5. 从 GitHub Release 下载 npm tarball
6. 通过 OIDC 执行 `npm publish`

## 最小可发布清单

正式发布前，至少确认以下事项：

- `sohaha/nmtx` 已配置 `CODEBAY_RELEASE_WRITE_TOKEN`
- `sohaha/nmtx` 已配置 `CNB_TOKEN`
- npm 包 `@sohaha/zcodex` 已配置 trusted publisher
- 如果发布 macOS，相关 Apple 签名 secrets 已配置
- 触发命令中的版本号符合规则

## 一条命令发布示例

稳定版：

```bash
mise run trigger-nmtx-codex-ci --version 1.2.3
```

alpha 版：

```bash
mise run trigger-nmtx-codex-ci --version 1.2.3-alpha.1
```
