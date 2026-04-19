# ztok UTF-8 截断与成对参数剥离

## 背景
- 在修复 RTK/ztok 升级后的 review findings 时，`ztok json` 新增的默认值视图和 `ztok vitest` 新参数形态都出现了边界问题。

## 结论
- 文本压缩/预览逻辑里，只要会截断用户可见字符串，就不能用固定字节下标切 `String`；对中文、emoji 等多字节字符，必须按 `chars()` 或其他字符边界安全方式截断。
- 删除冗余 CLI 选项时，若该选项支持 `--flag value` 形态，不能只过滤 flag 本身；必须连带消费下一个值，否则残留 value 会被错当成位置参数，改变真实执行语义。

## 在 ztok 中的落点
- `codex-rs/ztok/src/json_cmd.rs`
  - 紧凑 JSON 值视图对长字符串应按字符数截断。
  - 列表/对象摘要提示不能输出 `... +0 个键` 这类伪剩余信息。
- `codex-rs/ztok/src/vitest_cmd.rs`
  - 清洗用户传入的 `run` / `watch` / `--watch*` / `--reporter*` 时，要同时覆盖 `--reporter=value` 与 `--reporter value` 两种形态。

## 最小验证
- `cargo test -p codex-ztok`
- `cargo test -p codex-cli --test ztok ztok_vitest_drops_reporter_value_pair -- --exact`
