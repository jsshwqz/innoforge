#!/bin/bash
# M-B 规格书审查修正脚本（Issue #30 配套）
# 用法: bash apply-spec-fix.sh path/to/2026-09-28-mb-construction-spec.md
# 审查人：架构伙伴 AI | 日期：2026-09-28 | Issue: #30
set -euo pipefail
F="${1:?usage: $0 <spec.md>}"

# 修正 1: compute_and_save_embedding 行号 166 → 159
# 原文：vector/mod.rs:166 | 实际：vector/mod.rs:159
sed -i 's|vector/mod.rs:166|vector/mod.rs:159|g' "$F"

# 修正 2: 截断点从"两处"改为"八处" + 完整列表
# 原文只列 analysis.rs:50 和 deep_reasoning.rs:138，实际有 8 处 chars().take
sed -i 's|- 两处截断\*\*违反 AGENTS.md §2.5 的数据完整性纪律\*\*：`analysis.rs:50` 与 `deep_reasoning.rs:138` 用 `chars().take(150/120)`，不走 `ai::client::truncate_for_ai`（`:267`，带完整性提示的既有工具）。|- **八处** `chars().take` 截断**违反 AGENTS.md §2.5 的数据完整性纪律**（不走 `ai::client::truncate_for_ai`（`:267`，带完整性提示的既有工具））：\n  - `analysis.rs:50`（snippet→AI prompt，150 字符）、`:156`（ai_analysis→AI prompt，500）、`:180`（snippet→FeatureCard description，300）、`:193`（snippet→core_structure，200）、`:250`（ai_analysis→AI prompt，2000）\n  - `deep_reasoning.rs:138`（snippet→AI prompt，120）\n  - `oa_response.rs:137`（snippet→AI prompt，80）\n  - `claim_tree.rs:24`（ai_analysis→AI prompt，800）\n  \n  其中 `analysis.rs:50`/`:180`/`:193`、`deep_reasoning.rs:138`、`oa_response.rs:137` 截断的是专利摘要喂给 AI 的内容；`analysis.rs:156`/`:250`、`claim_tree.rs:24` 截断的是 AI 自身产出再喂回。两类都违反 §2.5「传给 AI 的数据必须保留全文」纪律。|' "$F"

# 修正 3: MB0 追加 VectorIndex 架构提示
# compute_and_save_embedding 需 &VectorIndex，而 insert_patent 只有 &Database
sed -i 's|③ `compute_and_save_embedding` 接上真实调用点——\*\*只接「新入库顺手算」\*\*（挂在 `db/patent.rs::insert_patent` 之后的单一写入口侧），\*\*禁止\*\*在本包做全表批量回填（4.3GB 用户库，风险与耗时都不可控）；④|③ `compute_and_save_embedding` 接上真实调用点——**只接「新入库顺手算」**（挂在 `db/patent.rs::insert_patent` 之后的单一写入口侧），**禁止**在本包做全表批量回填（4.3GB 用户库，风险与耗时都不可控）；\n  - ⚠️ **架构提示**：`compute_and_save_embedding` 签名需要 `\\&VectorIndex`，而 `insert_patent`（`db/patent.rs:7`）只有 `\\&self`（Database），`AppState`（`routes/mod.rs:385`）也不持有 `VectorIndex`。执行棒须在以下方案中选一并落注释：(a) 在调用方于 `insert_patent` 返回后用默认参数构造 `VectorIndex` 再调——**推荐**；(b) 在 `insert_patent` 内部内联。embedding 失败**不得回滚**专利入库，失败只 `tracing::warn!` 记账；④|' "$F"

# 修正 4: MB3 "两处同批改" → "八处同批改"
sed -i 's|③ `analysis.rs:50` / `deep_reasoning.rs:138` 的 `chars().take` 改走 `truncate_for_ai`，且\*\*两处同批改\*\*（改一处另一处仍违规）|③ **全部八处** `chars().take` 截断改走 `truncate_for_ai`（`analysis.rs:50/156/180/193/250`、`deep_reasoning.rs:138`、`oa_response.rs:137`、`claim_tree.rs:24`），**八处同批改**（少改一处则该处仍违规）|' "$F"

echo "Done. 4 fixes applied. Verify with: git diff $F"
echo "Expected changes:"
echo "  1. Line ~32: 166 → 159"
echo "  2. Line ~40: 两处 → 八处 + full list"
echo "  3. Line ~57: +架构提示"
echo "  4. Line ~75: 两处同批改 → 八处同批改"
