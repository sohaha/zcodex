## 2026-04-22 `/models` 返回 200 但 schema 不兼容时，也应持久化禁用主 provider 刷新

### 背景

这次排查自定义 provider 启动慢时，旧修复已经覆盖了 `GET /models` 返回 `404` / `405` / `501` 的情况：

- 当前进程内停止继续请求主 provider 的 `/models`
- 把 unsupported 状态写到 `codex_home`，避免下个进程重启后再先打一枪
- 直接走 fallback provider 更新模型目录

但新的现场不是 unsupported status，而是：

- 主 provider 的 `/models` 返回 `200 OK`
- body 结构是 `{"data":[...],"has_more":false}`
- Codex 期望的结构仍然是 `{"models":[...]}`
- 解码阶段抛出 `failed to decode models response: missing field \`models\``

### 这次确认的事实

- 这种失败会被 `codex_api::map_api_error` 映射成 `CodexErr::Stream(...)`，不是 `UnexpectedStatus`。
- 旧逻辑只对 `UnexpectedStatus(404/405/501)` 触发持久化禁用，因此 200 + schema mismatch 不会命中 fallback。
- 如果只做“当前这次请求失败后临时回退”，而不把它并入持久化禁用路径，下次进程重启仍会先请求主 provider 的 `/models`，启动慢问题会持续存在。

### 这次形成的原则

- 对自定义 provider 的 `/models`，只要能确认它与 Codex 预期 schema 不兼容，就应与“不支持 `/models`”视为同类能力不兼容：
  - 当次刷新直接走 fallback provider
  - 持久化 provider signature，后续进程跳过主 provider `/models`
- 这类兼容性判断要收紧到稳定可识别的 schema 错误，不要把所有 `CodexErr::Stream` 一概当成能力不兼容。
- `500`、超时、连接错误、截断响应等仍属于真实故障，不能借 fallback 掩盖。

### 本次落地边界

- 当前只把 `failed to decode models response` 且包含 `missing field \`models\`` 的情况并入持久化禁用路径。
- 这足以覆盖 OpenAI 兼容 `/models` 常见的 `data` 包装响应，同时避免把临时性流错误误判成 schema 不兼容。

### 后续检查点

以后看到“models cache version mismatch 后每次启动都慢一下”，除了检查 `/models` 是否 `404` 外，还要补看：

1. `/models` 是否返回 `200` 但 body 结构不是 `ModelsResponse`
2. 该错误最终有没有落到持久化的 refresh-state 文件
3. 第二个进程是否仍会先请求主 provider `/models`
