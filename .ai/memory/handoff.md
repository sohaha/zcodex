# 交接记录

## 当前焦点

- 更新时间：2026-04-02T07:06:52.094Z
- 本轮摘要：已完成 app-server zmemory 路径对齐收尾：补齐 turn cwd override 下 zmemory handler、稳定偏好主动写入、以及子线程 spawn/resume/agent_jobs 的 project-scoped zmemory 重载，并把未做的全局 project-layer config 通用重载记录为已知风险。

## 待确认问题

- 暂无，后续若有其它 project-scoped config 直接消费 turn.config.*，需单独核对是否受同类问题影响。

## 下一步检查

- 后续新增 project-scoped 配置注入点时，先补 turn cwd override 场景验证
- 继续关注 system://workspace 的 dbPath/source/reason 是否与项目配置一致
