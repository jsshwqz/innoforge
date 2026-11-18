# 错误复盘索引（Errors Ledger Index）

> 为避免超长历史日志（>300KB）挤占 `src/context.rs` 的 6000 字符常驻记忆预算（`ERR-06`），自 `v1.0` 重构（`T-101`）起采用双层存储：
>
> 1. **常驻高频铁律摘要（≤4KB，供运行时自动注入）**：[`docs/errors-digest.md`](errors-digest.md)
> 2. **完整历史排障台账（#1～#79 全量归档）**：[`docs/archive/errors-2026-history.md`](archive/errors-2026-history.md)

## 新增复盘记录规范
- 凡涉及架构级普遍防线的踩坑，精炼 1～2 句提炼入 `docs/errors-digest.md`（保持总文件体积 ≤ 4096 字节）；
- 完整现场复盘（现象 / 根因 / 修复 / 负控）追加记录于下方或归档台账。

### [2026-11-18] T-101 规约换挡与 308KB 复盘台账分层治理
- **现象**：旧版 `docs/errors.md` 累积至 308KB，且 `src/context.rs` 未纳入 `product-vision.md` 六大永久公理与 `AGENTS.md` 核心铁律。
- **修复**：历史全文无损归档至 `docs/archive/errors-2026-history.md`，提炼 ≤4KB 的 `docs/errors-digest.md`，升级 `src/context.rs` 按优先级汇编 `AGENTS.md` + `product-vision.md` + `feedback.md` + `errors-digest.md` 且总输出严格 ≤6000 字符。
