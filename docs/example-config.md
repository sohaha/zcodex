# 示例配置

示例配置文件见 [此文档](https://developers.openai.com/codex/config-sample)。

## zmemory：编码记忆推荐样板

下面是一个更适合“项目知识库”场景的 `zmemory` 配置样板：

```toml
[zmemory]
# 默认可不写。
# 不写时，Codex 会按项目自动落到：
# $CODEX_HOME/zmemory/projects/<project-key>/zmemory.db
#
# 只有你明确想让多个项目共用一份库时，才改成全局绝对路径：
# path = "/absolute/path/to/.codex/zmemory/zmemory.db"

# 允许写入的业务域。
# system 是内置只读域，不需要、也不应该写进来。
valid_domains = ["core", "project", "notes"]

# 启动时优先关注的少量高价值 boot 锚点。
# 建议只放稳定的协作规则，不要把整份项目知识塞进 boot。
core_memory_uris = [
  "core://agent/coding_operating_manual",
  "core://my_user/coding_preferences",
  "core://agent/my_user/collaboration_contract",
]
```

推荐约定：

- `core://...`：长期稳定规则、协作约束、用户偏好
- `project://<repo>/...`：项目架构、模块地图、测试入口、常见坑
- `notes://...`：阶段性 debug 结论、迁移观察、待沉淀经验

补充说明：

- `[zmemory]` 配置优先级高于环境变量
- 默认仍是项目库，不会因为这份推荐样板自动变成全局库
- 如果你要跨项目共享数据库，再显式设置全局 `path`
