# Phase 2 — 端口层建设 ⭐枢纽（M2）

> 前置阅读：task-breakdown.md Phase 2 表格 + Phase 0.5 裁决；本阶段决定后续拆分是否"接端口"而非"造新轮"

- [ ] T2.1 SearchProvider trait 统一三套 SerpAPI 实现（steps/search.rs / routes/ai.rs:84 / routes/patent.rs:67）
- [ ] T2.2 Embedder 重写合一：char-boundary 安全统一向量模块（修 CJK panic + 排序语义缺陷），删 vector/mod.rs 与 search.rs 私有版两份拷贝；决断 patents_embedding 写入路径；补中文回归测试
- [ ] T2.3 ai/client.rs 拆 adapter：HttpOpenAI / AnthropicApi / GeminiCli + FailoverClient + 统一 StreamParser；容灾循环测试 1→≥8
- [ ] T2.4 ProviderConfig 表驱动：服务商知识三处归一（AppConfig 字段族/settings 映射/idea.rs 注册表副本 + settings.html 前端预设表对齐）
- [ ] T2.5 upload.rs 提取器策略化（PdfExtractor trait 六实现注册表）+ SSRF 防护独立 net_guard.rs

验收门禁：全套 DoD + SerpAPI 调用全仓唯一 + TF-IDF 实现全仓唯一
Notes:
- 已知缺陷证据：compute_char_tfidf_embedding 与 vector/mod.rs 同源的 `cleaned[i..i+n]` 字节切片在中文输入 panic（逻辑级验证：String 切片遇非 char boundary 必 panic）；embedding 按 value 排序丢失 token 身份
- FALLBACK_AI_{1-5}_* env 组零消费，T2.4 时一并清理 .env.example 注释
