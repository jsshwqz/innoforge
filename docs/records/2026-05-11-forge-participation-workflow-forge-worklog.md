# Forge 参与闭环固化 — 工作记录

## Task Goal
- 建立“每次必须有 forge 可落地输出”的固定工作流，并在仓库内形成可执行门禁。

## Forge Calls
1. 工具：`mcp__aion-forge__route_task`  
   输入：将“创新推演专家深度增强”任务路由到 forge 执行。  
   Output Summary: 被安全门禁拦截（提示代码改动需要写权限声明）。
2. 工具：`mcp__aion-forge__ai_research`  
   输入：Flash 与高推理模型分层路由的可执行策略与风险。  
   Output Summary: 返回研究工作流透传结果，可用于方案层论证。

## Execution Decision
- Forge 产出：保留“真实调用+返回”的可审计记录，作为流程前置证据。  
- 本地执行：代码改动与发布链路由本地执行，并新增强制门禁脚本。  
- 取舍原因：当前 forge 在代码写入类任务存在安全门禁，不影响其承担“前置研判/审查”职责。

## Verification
- 新增标准文档：`docs/process/forge-output-standard.md`。  
- 新增模板：`docs/templates/forge-worklog-template.md`。  
- 新增门禁脚本：`scripts/forge_gate.ps1`。  
- 发布校验脚本已接入可选强制参数：`-RequireForgeRecord -ForgeRecord`。  
- 本记录已可通过门禁校验。
