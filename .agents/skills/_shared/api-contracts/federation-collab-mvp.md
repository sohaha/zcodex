# Federation Collab MVP API Contract

## Goal

把现有 federation 从 `instance + inbox + TextTask/TextResult` 原语，提升为“主线程协调前端/后端双 worker”的产品化协作 API。

## Existing Reusable Base

- `thread/start` 已支持 `federation` 参数。
- daemon 已支持 `register/list/send/read/ack/cleanup`。
- bridge 已能把入站任务转成普通本地 turn。

MVP 不直接把这些低层命令暴露给终端用户，而是在 app-server v2 之上定义更高层 contract。

## Domain Model

### FederationCollabSession

```json
{
  "sessionId": "string",
  "ownerThreadId": "string",
  "title": "string",
  "participants": [
    {
      "threadId": "string",
      "instanceId": "string",
      "role": "frontend | backend | coordinator | custom",
      "displayName": "string",
      "status": "online | stale | offline",
      "cwd": "string",
      "lastHeartbeatAt": 1710000000,
      "lastMessageAt": 1710000001
    }
  ],
  "createdAt": 1710000000,
  "updatedAt": 1710000001
}
```

### FederationCollabMessage

```json
{
  "messageId": "string",
  "sessionId": "string",
  "senderThreadId": "string",
  "recipientThreadId": "string",
  "messageType": "task | message | result",
  "text": "string",
  "inReplyTo": "string | null",
  "createdAt": 1710000000,
  "status": "accepted | delivered | failed"
}
```

## RPC Contracts

### `federation/session/create`

创建协作 session，并注册主线程视角下的 worker 关系。

Request:

```json
{
  "title": "frontend-backend collab",
  "participants": [
    {
      "threadId": "thread-frontend",
      "instanceId": "inst-frontend",
      "role": "frontend",
      "displayName": "前端 Codex"
    },
    {
      "threadId": "thread-backend",
      "instanceId": "inst-backend",
      "role": "backend",
      "displayName": "后端 Codex"
    }
  ]
}
```

Response:

```json
{
  "session": {
    "sessionId": "sess-123",
    "ownerThreadId": "thread-main",
    "title": "frontend-backend collab",
    "participants": []
  }
}
```

### `federation/session/read`

读取单个协作 session 的当前状态。

Request:

```json
{
  "sessionId": "sess-123"
}
```

Response:

```json
{
  "session": {}
}
```

### `federation/session/list`

列出当前 owner thread 可见的协作 session。

Request:

```json
{
  "cursor": null,
  "limit": 20
}
```

Response:

```json
{
  "data": [],
  "nextCursor": null
}
```

### `federation/message/send`

发送协作消息。MVP 统一走高层 message contract，不直接暴露 envelope。

Request:

```json
{
  "sessionId": "sess-123",
  "senderThreadId": "thread-frontend",
  "recipientThreadId": "thread-backend",
  "messageType": "message",
  "text": "接口会多一个 status 字段，页面要不要展示？",
  "inReplyTo": null
}
```

Response:

```json
{
  "message": {}
}
```

### `federation/message/list`

按 session 拉取消息流。

Request:

```json
{
  "sessionId": "sess-123",
  "cursor": null,
  "limit": 50
}
```

Response:

```json
{
  "data": [],
  "nextCursor": null
}
```

## Notifications

### `federation/sessionUpdated`

当 participant 在线状态、最近消息时间、标题等变化时发出。

```json
{
  "sessionId": "sess-123",
  "session": {}
}
```

### `federation/messageReceived`

当任一 worker 发来 task/message/result 时发出。

```json
{
  "sessionId": "sess-123",
  "message": {}
}
```

### `federation/messageStatusChanged`

当消息从 accepted 变为 delivered 或 failed 时发出。

```json
{
  "sessionId": "sess-123",
  "messageId": "msg-123",
  "status": "delivered"
}
```

## MVP Rules

- `messageType=task`:
  - 表示新的分派任务。
- `messageType=message`:
  - 表示中途沟通，不视为任务完成。
- `messageType=result`:
  - 表示阶段性交付，默认需要回流到 owner thread。

- owner thread 必须能收到所有 `result`。
- owner thread 可配置是否旁路接收 `message`；MVP 默认接收并展示。
- session 恢复时至少恢复：
  - participant 绑定
  - 最近消息
  - 在线状态

## Compatibility Notes

- 底层 daemon 仍可沿用 envelope 持久化，但上层 app-server 不应要求客户端感知 envelope id。
- 现有 `TextTask/TextResult` 可作为实现细节继续存在，但产品 API 以 `messageType` 语义为准。
- 未来若扩展多 worker 或远程 federation，应保持 `sessionId` 与 `messageType` 语义稳定。
