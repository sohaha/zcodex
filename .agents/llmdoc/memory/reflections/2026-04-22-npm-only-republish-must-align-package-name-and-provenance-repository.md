# npm-only 重发必须同时对齐包名与 provenance 仓库字段

## 触发场景
- `sohaha/nmtx` 的 `codex.yml` 用 `publish_mode=npm-only` 从 `sohaha/zcodex` 的既有 GitHub Release 下载 `codex-npm*.tgz`，再直接执行 `npm publish --access public --provenance`。

## 这次踩坑
- 先失败在包名：release 上的 npm tarball 里仍是 `@sohaha/codex`，而目标应为 `@sohaha/zcodex`。
- 把 tarball 中的包名改成 `@sohaha/zcodex` 后再次失败，但根因变成 npm provenance 校验：
  `package.json.repository.url` 写成了 `https://github.com/sohaha/zcodex`，而实际发布它的 workflow 运行在 `https://github.com/sohaha/nmtx`，npm 返回 `E422`，明确要求仓库信息与 provenance 匹配。

## 稳定结论
- 只要 `npm-only` 复用的是既有 release tarball，就不能只改仓库源码后直接重触发；必须先重做并覆盖上传 release 上的 npm tgz。
- 这类重发不需要重新编译 Rust 二进制，只需重封装 npm tarball。
- 对启用 `--provenance` 的 npm 发布，tarball 里的 `package.json.repository.url` 必须与实际执行 `npm publish` 的 GitHub Actions 仓库一致；如果是 `sohaha/nmtx` 发包，就要写 `https://github.com/sohaha/nmtx`。

## 最小闭环
- 修改仓库里的 npm 包名与安装提示，使新生成的 tarball 使用正确包名。
- 基于既有 release 资产重封装 npm tgz。
- 覆盖上传 release 上的 npm 资产。
- 再触发 `mise run publish-npm --version <version>`。
