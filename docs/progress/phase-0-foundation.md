# Phase 0 — 地基与卫生

> 前置阅读：task-breakdown.md Phase 0 表格（含验收标准全文）+ Phase 0.5 死代码裁决

## 任务清单

- [x] T0.1 处置未提交改动（审阅→剥离调试→保留三处真修复）✅ commit 620b263
- [x] 预处理：patent_detail.html 合并冲突标记解决 ✅ commit c6e3139
- [x] 预处理：idea.html FreeCAD 接线恢复 ✅ commit c6e3139
- [x] T0.10 恢复本地门禁全绿 ✅ commit 413db81（fmt 应用/clippy 32→0/三处陈旧版本断言 18→23/mcp 暂允许+T6.4 标注；最终验证 fmt=0 clippy=0 test 369 通过 0 失败 HTML 扫描过）
- [ ] T0.2 工作区垃圾清理（4.1GB db / 118MB zip 先归档仓库外；游离 main.rs 删除；~250 临时文件直删）
- [ ] T0.3 SCHEMA_VERSION 22→23 对齐 + target 参数 warn 断言
- [ ] T0.4 文档漂移修正（AGENTS.md 双入口/16步/v11/AI超时/8页面 + ARCHITECTURE.md 模板引擎）
- [ ] T0.5 分支切换：checkout dev → merge main → 确认绿
- [ ] T0.6 推送 main+dev 至 GitHub/Gitee 双远端，CI 三 job 全绿（当前领先远端 60+/36 提交未经 CI！）
- [ ] T0.7 Dockerfile 修复：bin 名 innoforge→innoforge-server（两处）；删冗余 COPY；docker build 验证
- [ ] T0.8 functions-manifest --refresh（settings.html: formatNumber/loadAiCostStats 入基线）
- [ ] T0.9 【用户动作】.env 密钥轮换提醒与泄露扫描
- [ ] T0.10 **恢复本地门禁全绿**：①fmt 漂移已应用（ai/client.rs usage 解析段）；②clippy 32 错误清零（集中于 vector/mod.rs 等待重写区，机械修复 manual_clamp 类 lint）；③cargo test 恢复（根因是 D 盘满导致链接失败，cargo clean 后重跑）

## 验收门禁

fmt/clippy/test 全绿 + check_html_functions 通过 + （T0.6）双远端 CI 绿

## Notes

- 2026-08-15：cad_template_contract 曾红（idea.html 丢 CAD 接线、patent_detail 有冲突标记），已修复待门禁确认后提交。
- 根目录实测大件：innoforge.db=4136MB(+wal 3.9MB)、docs.zip=117.6MB、src.zip≈0.3MB、游离 main.rs=17KB（旧单体版，Cargo.toml 未引用）。
