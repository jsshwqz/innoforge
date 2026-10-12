# Forge 可落地输出标准（强制）

> 目的：确保每次任务都有 `aion-forge` 参与痕迹与可审计产物，不再出现“仅口头参与”。

---

## 1. 适用范围

- 任何进入实现/修复/发布链路的任务（代码、配置、发布、回滚）。

## 2. 最低合规要求（缺一不可）

1. 存在一份任务记录文件：`docs/records/YYYY-MM-DD-*-forge-worklog.md`
2. 记录中必须包含以下 4 个段落标题：
- `## Task Goal`
- `## Forge Calls`
- `## Execution Decision`
- `## Verification`
3. `Forge 调用记录` 中至少 1 条真实调用结果（成功或被拦截都可，但必须有返回内容）。
4. `落地决策` 必须明确“哪些由 forge 产出，哪些由本地执行”。

## 3. 推荐执行顺序

1. 先调用 forge 路由/研究能力，拿到建议或拦截结果。
2. 再执行本地实现。
3. 产出工作日志并附上关键结论。
4. 运行 `scripts/forge_gate.ps1` 做门禁校验。
5. 通过后才允许进入推送/发布流程。

## 4. 与发布校验的关系

- 发布前可启用：`scripts/release_verify.ps1 -RequireForgeRecord -ForgeRecord <path>`
- 未通过 forge 门禁视为流程不完整。
