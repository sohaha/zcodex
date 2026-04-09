# 2026-04-09 ztldr shared grep payloads 反思

## 背景
- `auto_tldr.rs` 的测试虽然已经共享了 query corpus / summary helper，但 `grep_files` 的 JSON payload 还散落在多处 `{"pattern":...,"include":...}` 字符串里。

## 本轮有效做法
- 在 `core/src/tools/rewrite/test_corpus.rs` 新增 `grep_payload(pattern, include)` helper。
- `auto_tldr.rs` 的 grep rewrite 相关测试统一通过该 helper 构造 payload，而不是继续手写 JSON 字符串。

## 关键收益
- payload 结构改动时不需要逐个测试搜字符串替换。
- `include` 可选这一层语义变成显式 helper 参数，比散落字符串更容易读和维护。

## 后续建议
- 如果后面再引入 read/grep 混合场景的 fixture，优先继续下沉 builder，不要回到拼 JSON 字符串。
