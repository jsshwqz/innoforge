# 2026-10-27 第 0 步账本：立账、停扩、对账

> 依据：`docs/plans/2026-10-27-rectification.md` §3。状态：已完成（2026-10-27）。
> 本文件只记可复核事实与指针，不含案件号、人名、公开号。

## 1. 一本账

- 唯一任务权威 = issue #90；最新实况 comment `6430486041`（2026-10-27 08:17 北京，M1 材料核对）。
- MASTER / STATUS / 2026-03-30 规划均为历史。MASTER §3 的「M-A 3/6、M-B 0/6、M-C 0/5」是 2026-09-21 快照，与 §6「M-A 已收口（PR #27，2026-09-28）」矛盾；以 §6 / PR 为准，§3 不再使用。
- 当前版本 `v0.9.20`（`Cargo.toml`）；main 已含 #125 的 `d048ed9`。

## 2. 冻结清单

| 项 | 实况 | 处置 |
|---|---|---|
| #78 chunks FTS5 + RRF | open，无分支 / 无 commit，云端未开工 | 暂停，M1 出分前不派 |
| #125 证据内核 v2 | PR closed（2026-10-27 18:28 北京），`d048ed9` 在 main；只接 idea Finalize（`src/pipeline/steps/finalize.rs:37-44`），OA 路由无调用 | 冻结，不扩；不得称为 OA「可追溯硬底座」 |
| #123（#77 busy_timeout） | PR open，未合并 | 保持 open，不新派；合并需授权 |
| #121(#83) / #122(#76) / #124 / #127 / #128 | 已合并 | 清算，不再列入待办 |
| #110 测试端口竞态 | open，测试基础设施 | 冻结（不服务产品质量） |

## 3. 质量门对账

- **M1 真实案件基线：未执行。** 没有五维分，没有 top-10 失败清单。
- 已跑的「M2-E 回归」（`/tmp/m2-regression/report.md`，2026-10-25）是**合成夹具**（`data/uploads/presubmit_simulated_fail.txt`）上的工程冒烟：fact_check 100 / anchor_rate 1 / quote_count 0 / decoded 811 字符 / 22.94s。证明链路通，不证明报告可交付。
- `v0.9.20` 的「M2 top-3」修的是 A 流式终止 / B 空文本网关 / C 来源标记（`CHANGELOG.md` v0.9.20），属工程修复；按本令**不计入 M1/M2 质量门**，不改变五维分。
- 结论：**M2 不得开工；下一步是 M1。**

## 4. M1 输入现状

- 副本库：`C:\Temp\m1-materials`（原库 `D:\test\patent-hub-backup\innoforge.db` 只读，原库零接触）。
- 可用：唯一 `reject_review` 记录（2026-08-15）；`analysis_snapshot` 10,875 字符（复审请求方案分析）；一笔 5,480 字符一通全文；`oa_text` 9,814 字符为驳回决定首页 OCR 残片。
- 唯一硬缺口：**驳回决定全文**（CPC 收件箱 / 电子件）。
- 模型通道：8080 网关进程在，`/v1/models` 返回 **401**（10-24 实测为 200）。
- 评分锚点：`D:\Temp\备料-今晚作业-2026-10-26.md`（本地，不入 git）。

## 5. 更正记录

- 5000 字截断：`src/ai/patent.rs:307-308`；`truncate_for_ai`（`src/ai/client.rs:297-308`，尾注 :304）会追加「材料不完整」尾注，**不是静默截断**。
- `fact_check_rejected`：`src/pipeline/steps/finalize.rs:30-32` 只 warn，不阻断；OA 生成前门在 `src/routes/ai.rs:1889`。
- #125 当前边界：OA 路由 grep `evidence_kernel` 零命中，仍只覆盖 idea。

## 6. 下一步（M1，阻塞如实记录）

1. 取驳回决定全文（用户 / CPC）。
2. 修 8080 的 401（key）。
3. 用同一案件跑完整链路，产出五维基线分 + top-10 失败清单 + 成本。
4. 门：≥3 维达 4/5 才进 M2。

不空等、不自造样本、不用合成夹具代替真实案件。

## 7. M1 已跑（2026-10-27）

- 模型通道：8080 用配置 key 恢复，`claude-opus-5-5` 可用（4.6 池 503）；fast 模型同。
- 用副本库唯一 `reject_review` 跑完整链路；输入为库内 OA 文本 + `analysis_snapshot` 背景（驳回决定全文仍缺）。
- 门未过：五维 2 / 2 / 2 / 1 / 1，0 维达 4/5。详见 `docs/plans/2026-10-27-m1-baseline.md`。
- 同轮修掉两个 P0：`src/ai/patent.rs:992` 字符边界 panic；`reject_review` 答复书因未传 `my_patent` 产出空文本却回 `[DONE]`。
- M2 只修 top-3：证据化+法条化 / CaseMeta 回填 / OA 输入完整性。
