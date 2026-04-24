# ZTeam 2026-04-24 架构审查笔记

## 一句话结论

ZTeam 现在已经是一个**可工作的本地 TUI 双 worker 模式**，但还不是一个**可平滑演进到多 transport 的稳定架构**。

## 必须守住的边界

1. TUI 继续做产品化工作台，不把当前需求下沉成新的 app-server 公共协议。
2. worker 身份不能只靠 slot 名称，必须再绑定一次具体运行代际。
3. federation 若要接入，先进入 `WorkerEndpoint` 这类本地 seam，不要直接污染现有 local thread 语义。

## 当前最危险的长期问题

- `/zteam start` 没有 generation，旧线程可能被重新认作新 worker。
- `FederationAdapter` 还是展示态对象，不是可执行 endpoint。
- ZTeam attach 复用了 picker 的 live attach helper，恢复边界和 UI 选择边界混在一起。

## 默认后续路线

先做：

- `run_id/generation`
- `WorkerBinding/WorkerEndpoint`
- 独立的 attach/materialize seam

后做：

- attach 候选索引优化
- 统一的状态摘要/阻塞原因派生模型

暂不做：

- 新 app-server RPC
- zfeder/federation 产品名或公共协议改写
