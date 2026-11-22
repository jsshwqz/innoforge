# API 契约（自动生成）

> 本文件由 `scripts/gen_api_contracts.js` 从 `src/common.rs` 的路由声明与处理函数源码生成，**不要手改**。
> `tests/api_contracts_up_to_date.rs` 会用同一脚本的 `--check` 模式比对，代码改了没重跑就红。
> 叙述性说明、示例与业务语义仍写在 [`docs/API.md`](API.md)。

共 **159** 条 `/api` 路由（另有 9 条页面/静态路由，见 [API-ROUTES.md](API-ROUTES.md)）。

## 怎么读

- **路径参数**：取自路由模板里的 `:name`；处理函数用 `Path<T>` 接。
- **请求体 / 查询**：写出类型名；类型能在 `src/` 里找到 `struct` 定义时，下文按字段列出（含 serde 重命名与是否可省略）。
- **任意 JSON**：端点的类型是 `serde_json::Value`，字段没有类型约束。本文件从函数体里**静态抽取**它实际读取的键与返回的顶层键；
  经共享 helper 读取的键可能抽不到——抽不到会写明「未抽取到」，不拿「没写」冒充「没有」。
- **响应**：能确定 `Json<T>` 的写类型；`Response`/`impl IntoResponse` 这类动态响应只标注形态，状态码与正文见源码行。

## 概览

| 维度 | 数量 |
| --- | --- |
| `/api` 路由 | 159 |
| 请求体是具体结构体 | 21 |
| 请求体是任意 JSON | 51 |
| 其中能从函数体抽到请求键 | 50 |
| 响应含可识别 JSON 顶层键 | 131 |
| 引用的请求/查询类型已解析到 struct | 22/22 |
| 处理函数未找到 | 0 |

## 路由契约总表

| # | 方法 | 路径 | 路径参数 | 请求体 | 查询 | 响应 | 处理函数 | 源码 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | GET | `/api/cad/status` | — | — | — | JSON（未抽到顶层键） | `routes::api_cad_status` | `src/routes/cad.rs:41` |
| 2 | POST | `/api/cad/start` | — | — | — | JSON（未抽到顶层键） | `routes::api_cad_start` | `src/routes/cad.rs:45` |
| 3 | POST | `/api/cad/draw` | — | `CadDrawRequest` | — | JSON CadDrawResponse | `routes::api_cad_draw` | `src/routes/cad.rs:49` |
| 4 | GET | `/api/cad/artifacts` | — | — | `CadHistoryQuery` | JSON（§4） | `routes::api_cad_artifacts` | `src/routes/cad.rs:165` |
| 5 | GET | `/api/cad/artifacts/:id/preview` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_cad_preview` | `src/routes/cad.rs:189` |
| 6 | GET | `/api/cad/artifacts/:id/download/:format` | `id`, `format` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_cad_download` | `src/routes/cad.rs:197` |
| 7 | GET | `/api/version` | — | — | — | JSON（§7） | `routes::api_get_version` | `src/routes/settings.rs:57` |
| 8 | GET | `/api/oa/deadline-table` | — | — | — | JSON（§8） | `crate::oa_deadline::deadline_table` | `src/oa_deadline.rs:158` |
| 9 | GET | `/api/oa/compute-deadline` | — | — | — | JSON（§9） | `crate::oa_deadline::compute_deadline_api` | `src/oa_deadline.rs:176` |
| 10 | GET | `/api/settings` | — | — | — | JSON（§10） | `routes::api_get_settings` | `src/routes/settings.rs:78` |
| 11 | POST | `/api/settings/cad` | — | `CadSettingsRequest` | — | JSON（§11） | `routes::api_save_cad_settings` | `src/routes/settings.rs:198` |
| 12 | POST | `/api/settings/search-agent` | — | 任意 JSON（§12） | — | JSON（§12） | `routes::api_save_search_agent_settings` | `src/routes/settings.rs:169` |
| 13 | POST | `/api/settings/serpapi` | — | 任意 JSON（§13） | — | JSON（§13） | `routes::api_save_serpapi` | `src/routes/settings.rs:472` |
| 14 | POST | `/api/settings/ai` | — | 任意 JSON（§14） | — | JSON（§14） | `routes::api_save_ai` | `src/routes/settings.rs:534` |
| 15 | GET / POST | `/api/settings/ai-budget` | — | POST 任意 JSON（§15） | — | JSON（§15） | `routes::api_get_ai_budget`<br>`routes::api_save_ai_budget` | `src/routes/settings.rs:744`<br>`src/routes/settings.rs:752` |
| 16 | GET | `/api/settings/serpapi/balance` | — | — | — | JSON（§16） | `routes::api_serpapi_balance` | `src/routes/settings.rs:794` |
| 17 | GET | `/api/chat/:session_key` | `session_key` | — | `ChatPagination` | JSON（§17） | `routes::api_chat_get_messages` | `src/routes/chat.rs:21` |
| 18 | POST | `/api/chat/:session_key/save` | `session_key` | 任意 JSON（§18） | — | JSON（§18） | `routes::api_chat_save_message` | `src/routes/chat.rs:53` |
| 19 | POST | `/api/chat/:session_key/delete` | `session_key` | — | — | JSON（§19） | `routes::api_chat_delete_messages` | `src/routes/chat.rs:90` |
| 20 | POST | `/api/search` | — | `SearchRequest` | — | JSON SearchResult | `routes::api_search` | `src/routes/search.rs:417` |
| 21 | POST | `/api/search/stats` | — | `SearchRequest` | — | JSON（§21） | `routes::api_search_stats` | `src/routes/search.rs:1480` |
| 22 | POST | `/api/search/export` | — | `SearchRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_export_csv` | `src/routes/search.rs:1544` |
| 23 | POST | `/api/search/export/xlsx` | — | `SearchRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_export_xlsx` | `src/routes/search.rs:1595` |
| 24 | POST | `/api/search/online` | — | `SearchRequest` | — | JSON（§24） | `routes::api_search_online` | `src/routes/search.rs:676` |
| 25 | POST | `/api/search/vector` | — | 任意 JSON（§25） | — | JSON（§25） | `routes::api_search_vector` | `src/routes/search.rs:1310` |
| 26 | POST | `/api/search/analyze` | — | 任意 JSON（§26） | — | JSON（§26） | `routes::api_ai_analyze_results` | `src/routes/ai.rs:847` |
| 27 | POST | `/api/patent/fetch` | — | `FetchPatentRequest` | — | JSON（§27） | `routes::api_fetch_patent` | `src/routes/patent.rs:18` |
| 28 | GET | `/api/patent/enrich/:id` | `id` | — | — | JSON（§28） | `routes::api_enrich_patent` | `src/routes/patent.rs:39` |
| 29 | GET | `/api/patent/enrich-free/:id` | `id` | — | — | JSON（§29） | `routes::api_enrich_patent_free` | `src/routes/patent.rs:303` |
| 30 | POST | `/api/patent/:id/extract-pdf-drawings` | `id` | — | — | JSON（§30） | `routes::api_extract_patent_pdf_drawings` | `src/routes/patent.rs:341` |
| 31 | GET | `/api/patents/backfill-status` | — | — | — | JSON（§31） | `routes::api_patents_backfill_status` | `src/routes/patent.rs:390` |
| 32 | POST | `/api/patents/backfill-index` | — | — | — | JSON（§32） | `routes::api_patents_backfill_index` | `src/routes/patent.rs:398` |
| 33 | GET | `/api/patent/pdf/:id` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_patent_pdf` | `src/routes/patent.rs:875` |
| 34 | POST | `/api/patent/pdf/extract-text` | — | — | — | JSON（§34） | `routes::api_patent_pdf_extract_text` | `src/routes/upload.rs:1725` |
| 35 | GET | `/api/patent/image-proxy` | — | — | 任意键值对（§35） | HTTP 响应（动态，状态码/正文见源码） | `routes::api_patent_image_proxy` | `src/routes/patent.rs:1202` |
| 36 | GET | `/api/patent/lookup/:number` | `number` | — | 任意键值对（§36） | JSON（§36） | `routes::api_patent_lookup` | `src/routes/patent.rs:478` |
| 37 | POST | `/api/patent/lookup-or-fetch` | — | 任意 JSON（§37） | — | JSON（§37） | `routes::api_patent_lookup_and_fetch` | `src/routes/patent.rs:649` |
| 38 | POST | `/api/patent/resolve-chinese-titles` | — | `ResolveTitlesRequest` | — | JSON（§38） | `routes::api_resolve_chinese_titles` | `src/routes/patent.rs:559` |
| 39 | GET | `/api/patent/similar/:id` | `id` | — | — | JSON（§39） | `routes::api_recommend_similar` | `src/routes/patent.rs:731` |
| 40 | GET / POST / PATCH | `/api/patent/:id/legal-status` | `id` | `UpdateLegalStatusRequest` | — | JSON（§40） | `routes::api_patent_legal_status`<br>`routes::api_update_patent_legal_status`<br>`routes::api_update_patent_legal_status` | `src/routes/patent.rs:1323`<br>`src/routes/patent.rs:1409`<br>`src/routes/patent.rs:1409` |
| 41 | GET | `/api/patent/:id/citation-graph` | `id` | — | — | JSON（§41） | `routes::api_patent_citation_graph` | `src/routes/patent.rs:1451` |
| 42 | POST | `/api/ai/chat` | — | `AiChatRequest` | — | JSON AiResponse | `routes::api_ai_chat` | `src/routes/ai.rs:440` |
| 43 | POST | `/api/ai/chat/stream` | — | `AiChatRequest` | — | SSE 事件流（text/event-stream） | `routes::api_ai_chat_stream` | `src/routes/ai.rs:357` |
| 44 | POST | `/api/ai/chat/conclusions` | — | 任意 JSON（§44） | — | JSON（§44） | `routes::api_ai_chat_conclusions` | `src/routes/ai.rs:626` |
| 45 | GET | `/api/auth/google/url` | — | — | — | JSON（§45） | `routes::api_google_auth_url` | `src/routes/auth.rs:387` |
| 46 | GET | `/api/auth/google/callback` | — | — | — | (axum::http::StatusCode, &'static str) | `routes::api_google_callback` | `src/routes/auth.rs:406` |
| 47 | POST | `/api/auth/google/exchange` | — | — | — | JSON（§47） | `routes::api_google_exchange_handler` | `src/routes/auth.rs:447` |
| 48 | GET | `/api/auth/google/status` | — | — | — | JSON（§48） | `routes::api_google_oauth_status` | `src/routes/auth.rs:549` |
| 49 | GET | `/api/auth/gcloud/status` | — | — | — | JSON（§49） | `routes::api_gcloud_status` | `src/routes/auth.rs:60` |
| 50 | POST | `/api/auth/gcloud/login` | — | — | — | JSON（§50） | `routes::api_gcloud_login` | `src/routes/auth.rs:116` |
| 51 | POST | `/api/ai/summarize` | — | `FetchPatentRequest` | — | JSON AiResponse | `routes::api_ai_summarize` | `src/routes/ai.rs:716` |
| 52 | POST | `/api/ai/compare` | — | 任意 JSON（§52） | — | JSON AiResponse | `routes::api_ai_compare` | `src/routes/ai.rs:781` |
| 53 | POST | `/api/ai/claims` | — | 任意 JSON（§53） | — | JSON（§53） | `routes::api_ai_claims_analysis` | `src/routes/ai.rs:924` |
| 54 | POST | `/api/ai/risk` | — | 任意 JSON（§54） | — | JSON（§54） | `routes::api_ai_risk_assessment` | `src/routes/ai.rs:953` |
| 55 | POST | `/api/ai/compare-matrix` | — | 任意 JSON（§55） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_compare_matrix` | `src/routes/ai.rs:1016` |
| 56 | POST | `/api/ai/batch-summarize` | — | 任意 JSON（§56） | — | JSON（§56） | `routes::api_ai_batch_summarize` | `src/routes/ai.rs:1102` |
| 57 | POST | `/api/ai/inventiveness-analysis` | — | 任意 JSON（§57） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_inventiveness_analysis` | `src/routes/ai.rs:1263` |
| 58 | POST | `/api/ai/office-action-response` | — | 任意 JSON（§58） | — | JSON（§58） | `routes::api_ai_office_action_response` | `src/routes/oa_core.rs:57` |
| 59 | POST | `/api/ai/office-action-response/stream` | — | 任意 JSON（§59） | — | SSE 事件流（text/event-stream） | `routes::api_ai_office_action_response_stream` | `src/routes/oa_core.rs:355` |
| 60 | POST | `/api/ai/oa-generate-response-letter` | — | 任意 JSON（§60） | — | SSE 事件流（text/event-stream） | `routes::api_ai_oa_generate_response_letter` | `src/routes/oa_core.rs:677` |
| 61 | POST | `/api/ai/oa-discuss` | — | 任意 JSON（§61） | — | SSE 事件流（text/event-stream） | `routes::api_ai_oa_discuss` | `src/routes/oa_core.rs:1023` |
| 62 | POST | `/api/ai/oa-fetch-refs` | — | 任意 JSON（§62） | — | JSON（§62） | `routes::api_ai_oa_fetch_refs` | `src/routes/oa_assist.rs:6` |
| 63 | POST | `/api/ai/oa-panel` | — | 任意 JSON（§63） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_panel` | `src/routes/oa_assist.rs:94` |
| 64 | POST | `/api/ai/oa-history-compare` | — | 任意 JSON（§64） | — | JSON（§64） | `routes::api_ai_oa_history_compare` | `src/routes/oa_assist.rs:275` |
| 65 | POST | `/api/ai/oa-examiner-preview` | — | 任意 JSON（§65） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_examiner_preview` | `src/routes/oa_assist.rs:343` |
| 66 | POST | `/api/ai/oa-defense-analysis` | — | 任意 JSON（§66） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_defense_analysis` | `src/routes/oa_assist.rs:390` |
| 67 | POST | `/api/ai/oa-strategy-recommend` | — | 任意 JSON（§67） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_strategy_recommend` | `src/routes/oa_assist.rs:445` |
| 68 | POST | `/api/ai/oa-quality-score` | — | 任意 JSON（§68） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_quality_score` | `src/routes/oa_assist.rs:483` |
| 69 | POST | `/api/ai/audit-checks` | — | 任意 JSON（§69） | — | JSON（§69） | `routes::api_ai_audit_checks` | `src/routes/audit.rs:17` |
| 70 | POST | `/api/ai/prohibition-check` | — | 任意 JSON（§70） | — | JSON（§70） | `routes::api_ai_prohibition_check` | `src/routes/audit.rs:289` |
| 71 | POST | `/api/ai/oa-claim-simulate` | — | 任意 JSON（§71） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_claim_simulate` | `src/routes/oa_assist.rs:644` |
| 72 | POST | `/api/ai/oa-collect-evidence` | — | 任意 JSON（§72） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_collect_evidence` | `src/routes/oa_assist.rs:795` |
| 73 | POST | `/api/ai/oa-adversarial` | — | 任意 JSON（§73） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_adversarial` | `src/routes/oa_assist.rs:867` |
| 74 | POST | `/api/ai/vision-figure-check` | — | 任意 JSON（§74） | — | (StatusCode, Json<serde_json::Value>) | `routes::api_ai_vision_figure_check` | `src/routes/oa_assist.rs:923` |
| 75 | POST | `/api/ai/oa-feature-matrix` | — | 任意 JSON（§75） | — | (StatusCode, Json<serde_json::Value>) | `routes::api_ai_oa_feature_matrix` | `src/routes/oa_assist.rs:974` |
| 76 | GET | `/api/oa/history/all` | — | — | — | JSON（§76） | `routes::api_oa_history_all` | `src/routes/oa_core.rs:1363` |
| 77 | GET | `/api/oa/history/:patent_number` | `patent_number` | — | — | JSON（§77） | `routes::api_oa_history` | `src/routes/oa_core.rs:1352` |
| 78 | GET | `/api/oa/history/detail/:id` | `id` | — | — | JSON（§78） | `routes::api_oa_history_detail` | `src/routes/oa_core.rs:1374` |
| 79 | POST | `/api/oa/history/:id/delete` | `id` | — | — | JSON（§79） | `routes::api_oa_history_delete` | `src/routes/oa_core.rs:1482` |
| 80 | GET | `/api/oa/history/:patent_number/diff` | `patent_number` | — | — | JSON（§80） | `routes::api_oa_history_diff` | `src/routes/oa_diff.rs:21` |
| 81 | GET | `/api/oa/discussions/:patent_number` | `patent_number` | — | — | JSON（§81） | `routes::api_oa_discussion_list` | `src/routes/oa_core.rs:1386` |
| 82 | GET | `/api/oa/discussions/:patent_number/:discussion_id` | `patent_number`, `discussion_id` | — | — | JSON（§82） | `routes::api_oa_discussion_get` | `src/routes/oa_core.rs:1397` |
| 83 | POST | `/api/oa/discussions/import` | — | 任意 JSON（§83） | — | JSON（§83） | `routes::api_oa_discussion_import` | `src/routes/oa_core.rs:1419` |
| 84 | POST | `/api/oa/export-docx` | — | 任意 JSON（§84） | — | JSON（§84） | `routes::api_oa_export_docx` | `src/routes/oa_core.rs:1599` |
| 85 | GET | `/api/oa/:case_id/export/docx` | `case_id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_oa_case_export_docx` | `src/routes/oa_export.rs:716` |
| 86 | POST | `/api/oa/export` | — | 任意 JSON | — | HTTP 响应（动态，状态码/正文见源码） | `routes::oa_export::api_oa_export_post` | `src/routes/oa_export.rs:313` |
| 87 | GET | `/api/oa/export/:filename` | `filename` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::oa_export::api_oa_export_get` | `src/routes/oa_export.rs:324` |
| 88 | GET | `/api/oa/rounds/:patent_id` | `patent_id` | — | — | JSON（§88） | `routes::api_oa_rounds_get` | `src/routes/oa_rounds.rs:15` |
| 89 | GET | `/api/oa/:case_id/rounds/diff` | `case_id` | — | — | JSON（§89） | `routes::api_oa_rounds_diff` | `src/routes/oa_rounds.rs:279` |
| 90 | POST | `/api/oa/rounds` | — | 任意 JSON（§90） | — | JSON（§90） | `routes::api_oa_rounds_create` | `src/routes/oa_rounds.rs:41` |
| 91 | PUT | `/api/oa/rounds/update/:id` | `id` | 任意 JSON（§91） | — | JSON（§91） | `routes::api_oa_rounds_update` | `src/routes/oa_rounds.rs:156` |
| 92 | GET | `/api/oa/timeline/:patent_id` | `patent_id` | — | — | JSON（§92） | `routes::api_oa_timeline` | `src/routes/oa_rounds.rs:231` |
| 93 | POST | `/api/ai/check-amendments` | — | 任意 JSON（§93） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_check_amendments` | `src/routes/oa_core.rs:1493` |
| 94 | POST | `/api/ai/threat-assessment` | — | 任意 JSON（§94） | — | JSON（§94） | `routes::api_ai_threat_assessment` | `src/routes/oa_core.rs:1530` |
| 95 | POST | `/api/ai/claim-chart` | — | 任意 JSON（§95） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_claim_chart` | `src/routes/oa_core.rs:1561` |
| 96 | GET | `/api/ai/cost` | — | — | — | JSON（§96） | `routes::api_ai_cost_summary` | `src/routes/oa_core.rs:1636` |
| 97 | GET | `/api/ai/cost/records` | — | — | — | JSON（§97） | `routes::api_ai_cost_records` | `src/routes/oa_core.rs:1647` |
| 98 | POST | `/api/ai/cost/record` | — | 任意 JSON（§98） | — | JSON（§98） | `routes::api_ai_cost_save` | `src/routes/oa_core.rs:1662` |
| 99 | POST | `/api/idea/submit` | — | `IdeaSubmitRequest` | — | JSON（§99） | `routes::api_idea_submit` | `src/routes/idea.rs:176` |
| 100 | POST | `/api/idea/analyze` | — | 任意 JSON（§100） | — | JSON（§100） | `routes::api_idea_analyze` | `src/routes/idea.rs:224` |
| 101 | POST | `/api/idea/pipeline` | — | 任意 JSON（§101） | — | JSON（§101） | `routes::api_idea_pipeline` | `src/routes/idea.rs:1019` |
| 102 | POST | `/api/ideas/batch-compare` | — | 任意 JSON（§102） | — | JSON（§102） | `routes::api_ideas_batch_compare` | `src/routes/idea_reports.rs:599` |
| 103 | GET | `/api/idea/list` | — | — | — | JSON（§103） | `routes::api_idea_list` | `src/routes/idea.rs:407` |
| 104 | GET | `/api/idea/:id` | `id` | — | — | JSON（§104） | `routes::api_idea_get` | `src/routes/idea.rs:375` |
| 105 | POST | `/api/idea/:id/delete` | `id` | — | — | JSON（§105） | `routes::api_idea_delete` | `src/routes/idea.rs:417` |
| 106 | GET | `/api/idea/:id/progress` | `id` | — | — | SSE 事件流（text/event-stream） | `routes::api_idea_progress` | `src/routes/idea.rs:1241` |
| 107 | POST | `/api/idea/:id/resume` | `id` | — | — | JSON（§107） | `routes::api_idea_resume` | `src/routes/idea.rs:1181` |
| 108 | GET / POST | `/api/idea/:id/research-state` | `id` | `ResearchStateUpdateRequest` | — | JSON（§108） | `routes::api_idea_research_state`<br>`routes::api_idea_research_state_update` | `src/routes/idea_ops.rs:11`<br>`src/routes/idea_ops.rs:30` |
| 109 | POST | `/api/idea/:id/redirect` | `id` | `RedirectRequest` | — | JSON（§109） | `routes::api_idea_redirect` | `src/routes/idea_ops.rs:51` |
| 110 | GET | `/api/idea/:id/report` | `id` | — | 任意键值对（§110） | JSON（§110） | `routes::api_idea_report` | `src/routes/idea_reports.rs:8` |
| 111 | GET | `/api/idea/:id/report.html` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_report_html` | `src/routes/idea_reports.rs:163` |
| 112 | GET | `/api/ai/models` | — | — | 任意键值对（§112） | JSON（§112） | `routes::list_ai_models` | `src/routes/idea_ops.rs:906` |
| 113 | POST | `/api/idea/:id/chat` | `id` | `IdeaChatRequest` | — | JSON（§113） | `routes::api_idea_chat` | `src/routes/idea.rs:554` |
| 114 | GET | `/api/idea/:id/evidence` | `id` | — | — | JSON（§114） | `routes::api_idea_evidence` | `src/routes/idea.rs:446` |
| 115 | GET | `/api/idea/:id/kernel-report` | `id` | — | — | JSON（§115） | `routes::api_idea_kernel_report` | `src/routes/idea.rs:463` |
| 116 | GET | `/api/idea/:id/messages` | `id` | — | — | JSON（§116） | `routes::api_idea_messages` | `src/routes/idea.rs:993` |
| 117 | GET | `/api/idea/:id/token-usage` | `id` | — | — | JSON（§117） | `routes::api_idea_token_usage` | `src/routes/idea.rs:982` |
| 118 | GET | `/api/idea/:id/chat/conclusions` | `id` | — | — | JSON（§118） | `routes::api_idea_chat_conclusions` | `src/routes/idea_reports.rs:474` |
| 119 | GET | `/api/idea/:id/chat/conversation` | `id` | — | — | JSON（§119） | `routes::api_idea_chat_conversation` | `src/routes/idea_reports.rs:453` |
| 120 | POST | `/api/idea/:id/summarize` | `id` | — | — | JSON（§120） | `routes::api_idea_summarize_discussion` | `src/routes/idea_reports.rs:542` |
| 121 | GET / POST | `/api/ideas/:id/feature-cards` | `id` | `CreateFeatureCardRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_get_feature_cards`<br>`routes::api_create_feature_card` | `src/routes/feature_cards.rs:16`<br>`src/routes/feature_cards.rs:50` |
| 122 | POST | `/api/ideas/:id/feature-cards/generate` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_generate_feature_cards` | `src/routes/feature_cards.rs:585` |
| 123 | GET | `/api/ideas/:id/delivery-pack/xlsx` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_delivery_pack_xlsx` | `src/routes/idea_reports.rs:1033` |
| 124 | POST | `/api/ideas/:id/control-prototype` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_control_prototype` | `src/routes/idea_reports.rs:1084` |
| 125 | POST | `/api/ideas/:id/fto-scan` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_fto_scan` | `src/routes/idea_reports.rs:1124` |
| 126 | GET | `/api/ideas/:id/archive/zip` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_archive_zip` | `src/routes/idea_reports.rs:1316` |
| 127 | GET | `/api/ideas/:id/paper/latex-bundle` | `id` | — | `LatexBundleQuery` | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_paper_latex_bundle` | `src/routes/idea_reports.rs:1796` |
| 128 | GET | `/api/ideas/:id/paper/docx` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_paper_docx` | `src/routes/idea_reports.rs:1835` |
| 129 | GET | `/api/feature-cards/diff` | — | — | `FeatureCardDiffQuery` | HTTP 响应（动态，状态码/正文见源码） | `routes::api_feature_card_diff` | `src/routes/feature_cards.rs:132` |
| 130 | PUT | `/api/ideas/:id/claims` | `id` | `ReplaceClaimsRequest` | — | impl axum::response::IntoResponse | `routes::api_idea_claims_replace` | `src/routes/idea_ops.rs:261` |
| 131 | POST | `/api/ideas/:id/claims/validate` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_claims_validate` | `src/routes/idea_ops.rs:617` |
| 132 | GET / POST | `/api/idea/:id/claim-tree` | `id` | `UpsertClaimNodeRequest` | — | JSON（§132） | `routes::api_idea_claim_tree`<br>`routes::api_idea_claim_node_upsert` | `src/routes/idea_ops.rs:164`<br>`src/routes/idea_ops.rs:457` |
| 133 | DELETE | `/api/idea/:id/claim-tree/:claim_id` | `id`, `claim_id` | — | — | impl axum::response::IntoResponse | `routes::api_idea_claim_node_delete` | `src/routes/idea_ops.rs:572` |
| 134 | POST | `/api/idea/:id/iterate` | `id` | — | — | JSON（§134） | `routes::api_idea_iterate` | `src/routes/idea_reports.rs:699` |
| 135 | GET | `/api/idea/:id/versions` | `id` | — | — | JSON（§135） | `routes::api_idea_versions` | `src/routes/idea_ops.rs:722` |
| 136 | GET | `/api/idea/:id/branches` | `id` | — | — | JSON（§136） | `routes::api_idea_branches` | `src/routes/idea_ops.rs:747` |
| 137 | GET | `/api/idea/:id/findings` | `id` | — | — | JSON（§137） | `routes::api_idea_findings` | `src/routes/idea_ops.rs:758` |
| 138 | GET / POST | `/api/idea/:id/memory` | `id` | POST 任意 JSON（§138） | — | JSON（§138） | `routes::api_idea_memory`<br>`routes::api_idea_memory_add` | `src/routes/idea_ops.rs:769`<br>`src/routes/idea_ops.rs:790` |
| 139 | DELETE | `/api/idea/:id/memory/:entry_id` | `id`, `entry_id` | — | — | JSON（§139） | `routes::api_idea_memory_delete` | `src/routes/idea_ops.rs:848` |
| 140 | GET | `/api/ipc/tree` | — | — | — | JSON（§140） | `routes::api_ipc_tree` | `src/routes/ipc.rs:22` |
| 141 | GET | `/api/ipc/:code/patents` | `code` | — | — | JSON（§141） | `routes::api_ipc_patents` | `src/routes/ipc.rs:122` |
| 142 | POST | `/api/patents/import` | — | `ImportRequest` | — | JSON（§142） | `routes::api_import_patents` | `src/routes/settings.rs:857` |
| 143 | POST | `/api/backup/export` | — | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_backup_export` | `src/routes/settings.rs:1569` |
| 144 | POST | `/api/backup/check` | — | 任意 JSON（§144） | — | JSON（§144） | `routes::api_backup_check` | `src/routes/settings.rs:1630` |
| 145 | GET / POST | `/api/collections` | — | POST 任意 JSON（§145） | — | JSON（§145） | `routes::api_list_collections`<br>`routes::api_create_collection` | `src/routes/collections.rs:28`<br>`src/routes/collections.rs:8` |
| 146 | DELETE | `/api/collections/:id` | `id` | — | — | JSON（§146） | `routes::api_delete_collection` | `src/routes/collections.rs:49` |
| 147 | GET | `/api/collections/:id/patents` | `id` | — | — | JSON（§147） | `routes::api_get_collection_patents` | `src/routes/collections.rs:92` |
| 148 | POST | `/api/collections/:id/add` | `id` | 任意 JSON（§148） | — | JSON（§148） | `routes::api_add_to_collection` | `src/routes/collections.rs:59` |
| 149 | DELETE | `/api/collections/:id/remove/:patent_id` | `id`, `patent_id` | — | — | JSON（§149） | `routes::api_remove_from_collection` | `src/routes/collections.rs:82` |
| 150 | GET / POST | `/api/patents/:id/tags` | `id` | POST 任意 JSON（§150） | — | JSON（§150） | `routes::api_get_patent_tags`<br>`routes::api_add_tag` | `src/routes/collections.rs:128`<br>`src/routes/collections.rs:102` |
| 151 | DELETE | `/api/patents/:id/tags/:tag` | `id`, `tag` | — | — | JSON（§151） | `routes::api_remove_tag` | `src/routes/collections.rs:118` |
| 152 | GET | `/api/patents/:id/collections` | `id` | — | — | JSON（§152） | `routes::api_get_patent_collections` | `src/routes/collections.rs:151` |
| 153 | GET | `/api/tags` | — | — | — | JSON（§153） | `routes::api_list_all_tags` | `src/routes/collections.rs:138` |
| 154 | POST | `/api/upload/compare` | — | — | — | JSON（§154） | `routes::api_upload_compare` | `src/routes/upload.rs:340` |
| 155 | POST | `/api/upload/extract` | — | — | — | JSON（§155） | `routes::api_upload_extract` | `src/routes/upload.rs:518` |
| 156 | POST | `/api/upload/extract/stream` | — | — | — | SSE 事件流（text/event-stream） | `routes::api_upload_extract_stream` | `src/routes/upload.rs:678` |
| 157 | POST | `/api/upload/pdf-store` | — | — | — | JSON（§157） | `routes::api_upload_pdf_store` | `src/routes/upload.rs:275` |
| 158 | GET | `/api/patent/:id/numeral-check` | `id` | — | — | JSON（§158） | `routes::api_patent_numeral_check` | `src/routes/patent.rs:414` |
| 159 | POST | `/api/ai/numeral-consistency-check` | — | 任意 JSON（§159） | — | JSON（§159） | `routes::api_ai_numeral_consistency_check` | `src/routes/patent.rs:441` |

## 请求 / 查询类型字段

### `AiChatRequest`（请求体；定义于 `src/types/chat.rs:11`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `message` | `String` | 否 |
| `patent_id` | `Option<String>` | 是 |
| `history` | `Vec<(String, String)>` | 是 |
| `web_search` | `bool` | 是 |
| `images` | `Vec<String>` | 是 |
| `system_prompt` | `Option<String>` | 是 |
| `preset_mode` | `bool` | 是 |

使用该类型的端点：`/api/ai/chat`、`/api/ai/chat/stream`

### `AiResponse`（响应；定义于 `src/types/chat.rs:76`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `content` | `String` | 否 |

使用该类型的端点：`/api/ai/chat`、`/api/ai/summarize`、`/api/ai/compare`

### `CadDrawRequest`（请求体；定义于 `src/patent.rs:76`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `context_kind` | `CadContextKind` | 否 |
| `context_id` | `String` | 否 |
| `prompt` | `String` | 否 |
| `conversation_context` | `String` | 是 |
| `parent_artifact_id` | `Option<String>` | 是 |

使用该类型的端点：`/api/cad/draw`

### `CadDrawResponse`（响应；定义于 `src/patent.rs:101`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `artifact` | `CadArtifact` | 否 |
| `warnings` | `Vec<String>` | 是 |

使用该类型的端点：`/api/cad/draw`

### `CadHistoryQuery`（查询；定义于 `src/routes/cad.rs:29`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `context_kind` | `CadContextKind` | 否 |
| `context_id` | `String` | 否 |

使用该类型的端点：`/api/cad/artifacts`

### `CadSettingsRequest`（请求体；定义于 `src/routes/settings.rs:8`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `workspace` | `String` | 否 |
| `auto_start` | `bool` | 否 |

使用该类型的端点：`/api/settings/cad`

### `ChatPagination`（查询；定义于 `src/routes/chat.rs:15`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `limit` | `Option<usize>` | 是 |
| `offset` | `Option<usize>` | 是 |

使用该类型的端点：`/api/chat/:session_key`

### `CreateFeatureCardRequest`（请求体；定义于 `src/types/idea.rs:135`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `String` | 否 |
| `description` | `String` | 是 |
| `novelty_score` | `Option<f64>` | 是 |
| `technical_problem` | `String` | 是 |
| `core_structure` | `String` | 是 |
| `key_relations` | `String` | 是 |
| `process_steps` | `String` | 是 |
| `application_scenarios` | `String` | 是 |

使用该类型的端点：`/api/ideas/:id/feature-cards`

### `FeatureCardDiffQuery`（查询；定义于 `src/routes/feature_cards.rs:120`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `a` | `Option<String>` | 是 |
| `b` | `Option<String>` | 是 |
| `semantic` | `Option<bool>` | 是 |

使用该类型的端点：`/api/feature-cards/diff`

### `FetchPatentRequest`（请求体；定义于 `src/types/patent.rs:107`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `patent_number` | `String` | 否 |
| `source` | `Option<String>` | 是 |

使用该类型的端点：`/api/patent/fetch`、`/api/ai/summarize`

### `IdeaChatRequest`（请求体；定义于 `src/types/idea.rs:40`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `message` | `String` | 否 |
| `depth` | `String` | 是 |
| `images` | `Vec<String>` | 是 |
| `attachments` | `Vec<TextAttachment>` | 是 |

使用该类型的端点：`/api/idea/:id/chat`

### `IdeaSubmitRequest`（请求体；定义于 `src/types/idea.rs:26`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `String` | 否 |
| `description` | `String` | 否 |
| `input_type` | `String` | 是 |

使用该类型的端点：`/api/idea/submit`

### `ImportRequest`（请求体；定义于 `src/types/patent.rs:113`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `patents` | `Vec<Patent>` | 否 |

使用该类型的端点：`/api/patents/import`

### `LatexBundleQuery`（查询；定义于 `src/routes/idea_reports.rs:1790`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `template` | `Option<String>` | 是 |

使用该类型的端点：`/api/ideas/:id/paper/latex-bundle`

### `RedirectRequest`（请求体；定义于 `src/routes/idea.rs:38`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `restart_from` | `Option<String>` | 是 |
| `technical_domain` | `Option<String>` | 是 |
| `add_queries` | `Option<Vec<String>>` | 是 |
| `reason` | `Option<String>` | 是 |
| `research_state` | `Option<ResearchStateUpdateRequest>` | 是 |

使用该类型的端点：`/api/idea/:id/redirect`

### `ReplaceClaimsRequest`（请求体；定义于 `src/routes/idea_ops.rs:255`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `claims` | `Vec<UpsertClaimNodeRequest>` | 是 |

使用该类型的端点：`/api/ideas/:id/claims`

### `ResearchStateUpdateRequest`（请求体；定义于 `src/routes/idea.rs:30`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `current_hypothesis` | `Option<String>` | 是 |
| `excluded_paths` | `Option<Vec<String>>` | 是 |
| `open_questions` | `Option<Vec<String>>` | 是 |
| `verified_claims` | `Option<Vec<String>>` | 是 |

使用该类型的端点：`/api/idea/:id/research-state`

### `ResolveTitlesRequest`（请求体；定义于 `src/routes/patent.rs:548`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `patents` | `Vec<ResolveTitleItem>` | 否 |

使用该类型的端点：`/api/patent/resolve-chinese-titles`

### `SearchRequest`（请求体；定义于 `src/types/search.rs:23`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `query` | `String` | 否 |
| `page` | `usize` | 是 |
| `page_size` | `usize` | 是 |
| `country` | `Option<String>` | 是 |
| `date_from` | `Option<String>` | 是 |
| `date_to` | `Option<String>` | 是 |
| `search_type` | `Option<String>` | 是 |
| `sort_by` | `Option<String>` | 是 |
| `ipc` | `Option<String>` | 是 |
| `cpc` | `Option<String>` | 是 |
| `region` | `Option<String>` | 是 |
| `language` | `Option<Lang>` | 是 |
| `assignee` | `Option<String>` | 是 |
| `exact_assignee` | `bool` | 是 |

使用该类型的端点：`/api/search`、`/api/search/stats`、`/api/search/export`、`/api/search/export/xlsx`、`/api/search/online`

### `SearchResult`（响应；定义于 `src/types/search.rs:68`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `patents` | `Vec<PatentSummary>` | 否 |
| `total` | `usize` | 否 |
| `page` | `usize` | 否 |
| `page_size` | `usize` | 否 |
| `search_type` | `Option<String>` | 是 |
| `matched_fields` | `Option<Vec<String>>` | 是 |
| `applied_filter` | `Option<String>` | 是 |
| `reason` | `Option<String>` | 是 |
| `categories` | `Option<Vec<CategoryGroup>>` | 是 |
| `dedup_removed` | `usize` | 是 |

使用该类型的端点：`/api/search`

### `UpdateLegalStatusRequest`（请求体；定义于 `src/routes/patent.rs:1404`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `legal_status` | `String` | 否 |

使用该类型的端点：`/api/patent/:id/legal-status`

### `UpsertClaimNodeRequest`（请求体；定义于 `src/routes/idea_ops.rs:233`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `Option<String>` | 是 |
| `claim_number` | `u32` | 否 |
| `claim_type` | `String` | 是 |
| `parent_claim_id` | `Option<String>` | 是 |
| `content` | `String` | 否 |

使用该类型的端点：`/api/idea/:id/claim-tree`

## 嵌套类型（只展开一层）

上面的类型字段里直接引用的结构体与枚举。再深一层不展开，避免把整棵类型树都倒进来。

### `CadArtifact`（struct；被 `CadDrawResponse` 引用；定义于 `src/patent.rs:60`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `String` | 否 |
| `context_kind` | `CadContextKind` | 否 |
| `context_id` | `String` | 否 |
| `parent_artifact_id` | `Option<String>` | 是 |
| `revision` | `i64` | 否 |
| `prompt` | `String` | 否 |
| `assumptions` | `Vec<String>` | 否 |
| `preview_rel_path` | `String` | 否 |
| `fcstd_rel_path` | `String` | 否 |
| `step_rel_path` | `Option<String>` | 是 |
| `validation` | `CadValidation` | 否 |
| `created_at` | `String` | 否 |

### `CategoryGroup`（struct；被 `SearchResult` 引用；定义于 `src/types/search.rs:94`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `label` | `String` | 否 |
| `count` | `usize` | 否 |

### `Patent`（struct；被 `ImportRequest` 引用；定义于 `src/types/patent.rs:10`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `String` | 是 |
| `patent_number` | `String` | 否 |
| `title` | `String` | 否 |
| `abstract_text` | `String` | 是 |
| `description` | `String` | 是 |
| `claims` | `String` | 是 |
| `applicant` | `String` | 是 |
| `inventor` | `String` | 是 |
| `filing_date` | `String` | 是 |
| `publication_date` | `String` | 是 |
| `grant_date` | `Option<String>` | 是 |
| `ipc_codes` | `String` | 是 |
| `cpc_codes` | `String` | 是 |
| `priority_date` | `String` | 是 |
| `country` | `String` | 是 |
| `kind_code` | `String` | 是 |
| `family_id` | `Option<String>` | 是 |
| `legal_status` | `String` | 是 |
| `citations` | `String` | 是 |
| `cited_by` | `String` | 是 |
| `source` | `String` | 是 |
| `raw_json` | `String` | 是 |
| `created_at` | `String` | 是 |
| `images` | `String` | 是 |
| `pdf_url` | `String` | 是 |

### `PatentSummary`（struct；被 `SearchResult` 引用；定义于 `src/types/search.rs:104`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `String` | 否 |
| `patent_number` | `String` | 否 |
| `title` | `String` | 否 |
| `abstract_text` | `String` | 否 |
| `applicant` | `String` | 否 |
| `inventor` | `String` | 否 |
| `filing_date` | `String` | 否 |
| `country` | `String` | 否 |
| `relevance_score` | `Option<f64>` | 是 |
| `score_source` | `Option<String>` | 是 |
| `matched_fields` | `Option<Vec<String>>` | 是 |

### `ResolveTitleItem`（struct；被 `ResolveTitlesRequest` 引用；定义于 `src/routes/patent.rs:553`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `String` | 否 |
| `patent_number` | `Option<String>` | 是 |

### `TextAttachment`（struct；被 `IdeaChatRequest` 引用；定义于 `src/types/idea.rs:34`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `name` | `String` | 否 |
| `content` | `String` | 否 |

### `CadContextKind`（enum；被 `CadDrawRequest`、`CadHistoryQuery` 引用；定义于 `src/patent.rs:34`）

取值：`idea`（Rust: `Idea`）、`patent`（Rust: `Patent`）、`oa`（Rust: `Oa`）

### `Lang`（enum；被 `SearchRequest` 引用；定义于 `src/search/model.rs:16`）

取值：`chinese`（Rust: `Chinese`）、`english`（Rust: `English`）、`all`（Rust: `All`）

## 字段抽取明细

每条路由只列**能从源码里直接看到**的东西：函数体内读取的键（`req["x"]` / `req.get("x")` / `get_*(&req, "x")`）
与 `json!({...})` 返回的**顶层键**。嵌套结构、经共享 helper 读取的键、运行时拼接的键不在此列——抽不到就写「未抽取到」。

### §4 GET `/api/cad/artifacts`

- 源码：`src/routes/cad.rs:165`
- 响应顶层键：`artifacts`

### §7 GET `/api/version`

- 源码：`src/routes/settings.rs:57`
- 响应顶层键：`build_time`、`db_path`、`git_branch`、`git_commit`、`templates`、`version`

### §8 GET `/api/oa/deadline-table`

- 源码：`src/oa_deadline.rs:158`
- 响应顶层键：`deadline_table`、`kind`、`valid_types`、`value`

### §9 GET `/api/oa/compute-deadline`

- 源码：`src/oa_deadline.rs:176`
- 响应顶层键：`error`

### §10 GET `/api/settings`

- 源码：`src/routes/settings.rs:78`
- 响应顶层键：`ai_api_key`、`ai_api_key_anthropic`、`ai_api_key_configured`、`ai_api_key_deepseek`、`ai_api_key_gemini`、`ai_api_key_openai`、`ai_api_key_openrouter`、`ai_api_key_qwen`、`ai_api_key_sensetime`、`ai_api_key_xiaomi`、`ai_api_key_zhipu`、`ai_base_url`、`ai_model`、`ai_model_expert`、`aioncad_workspace`、`epo_configured`、`epo_key`、`epo_key_configured`、`epo_secret`、`epo_secret_configured`、`freecad_auto_start`、`freecad_platform_supported`、`gemini_cli_available`、`gemini_cli_enabled`、`gemini_cli_path`、`google_auth_mode`、`google_client_id`、`google_client_id_set`、`google_client_secret_set`、`google_oauth_connected`、`is_ollama`、`search_llm_daily_budget`、`search_rerank_enabled`、`serpapi_key_configured`、`serpapi_keys`、`umi_ocr_base_url`

### §11 POST `/api/settings/cad`

- 源码：`src/routes/settings.rs:198`
- 请求体：`CadSettingsRequest`（字段见上文类型表）
- 响应顶层键：`code`、`status`
- `status` 取值：error / ok

### §12 POST `/api/settings/search-agent`

- 源码：`src/routes/settings.rs:169`
- 读取的请求键：`daily_budget`、`rerank_enabled`
- 响应顶层键：`code`、`daily_budget`、`rerank_enabled`、`status`
- `status` 取值：error / ok

### §13 POST `/api/settings/serpapi`

- 源码：`src/routes/settings.rs:472`
- 读取的请求键：`api_keys`、`epo_key`、`epo_secret`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §14 POST `/api/settings/ai`

- 源码：`src/routes/settings.rs:534`
- 读取的请求键：`ai_model_expert`、`api_key`、`base_url`、`gemini_cli_enabled`、`google_client_id`、`google_client_secret`、`model`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §15 GET / POST `/api/settings/ai-budget`

- **GET** `routes::api_get_ai_budget`（`src/routes/settings.rs:744`）
  - 响应顶层键：`message`、`status`
  - `status` 取值：error
- **POST** `routes::api_save_ai_budget`（`src/routes/settings.rs:752`）
  - 读取的请求键：`daily_max_calls`、`daily_max_tokens`、`idea_max_tokens`
  - 响应顶层键：`message`、`status`
  - `status` 取值：error / ok

### §16 GET `/api/settings/serpapi/balance`

- 源码：`src/routes/settings.rs:794`
- 响应顶层键：`key_count`、`message`、`plan_name`、`remaining`、`searches_per_month`、`status`、`this_month_usage`、`total_usage`
- `status` 取值：error / ok

### §17 GET `/api/chat/:session_key`

- 源码：`src/routes/chat.rs:21`
- 响应顶层键：`limit`、`message`、`messages`、`offset`、`status`、`total`
- `status` 取值：error / ok

### §18 POST `/api/chat/:session_key/save`

- 源码：`src/routes/chat.rs:53`
- 读取的请求键：`content`、`role`
- 响应顶层键：`message`、`message_id`、`status`
- `status` 取值：error / ok

### §19 POST `/api/chat/:session_key/delete`

- 源码：`src/routes/chat.rs:90`
- 响应顶层键：`deleted`、`message`、`status`
- `status` 取值：error / ok

### §21 POST `/api/search/stats`

- 源码：`src/routes/search.rs:1480`
- 请求体：`SearchRequest`（字段见上文类型表）
- 响应顶层键：`applicants`、`countries`、`total`、`years`

### §24 POST `/api/search/online`

- 源码：`src/routes/search.rs:676`
- 请求体：`SearchRequest`（字段见上文类型表）
- 响应顶层键：`attempts`、`message`、`page`、`page_size`、`patents`、`source`、`total`

### §25 POST `/api/search/vector`

- 源码：`src/routes/search.rs:1310`
- 读取的请求键：`limit`、`query`、`rrf_k`
- 响应顶层键：`abstract_text`、`applicant`、`bm25_count`、`country`、`error`、`filing_date`、`fused_score`、`id`、`index_status`、`inventor`、`ipc_codes`、`matched_section_tag`、`matched_snippet`、`method`、`page`、`page_size`、`patent_number`、`patents`、`publication_date`、`relevance_score`、`rrf_k`、`score_source`、`source`、`title`、`total`、`vector_count`、`vector_score`

### §26 POST `/api/search/analyze`

- 源码：`src/routes/ai.rs:847`
- 读取的请求键：`patents`、`query`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §27 POST `/api/patent/fetch`

- 源码：`src/routes/patent.rs:18`
- 请求体：`FetchPatentRequest`（字段见上文类型表）
- 响应顶层键：`message`、`patent`、`status`
- `status` 取值：error / ok

### §28 GET `/api/patent/enrich/:id`

- 源码：`src/routes/patent.rs:39`
- 响应顶层键：`message`、`patent`、`status`
- `status` 取值：error / ok

### §29 GET `/api/patent/enrich-free/:id`

- 源码：`src/routes/patent.rs:303`
- 响应顶层键：`message`、`patent`、`remaining_secs`、`status`
- `status` 取值：cooldown / error / ok

### §30 POST `/api/patent/:id/extract-pdf-drawings`

- 源码：`src/routes/patent.rs:341`
- 响应顶层键：`count`、`images`、`message`、`pdf_url`、`status`
- `status` 取值：error / ok

### §31 GET `/api/patents/backfill-status`

- 源码：`src/routes/patent.rs:390`
- 响应顶层键：`message`、`status`
- `status` 取值：error

### §32 POST `/api/patents/backfill-index`

- 源码：`src/routes/patent.rs:398`
- 响应顶层键：`message`、`status`
- `status` 取值：error

### §34 POST `/api/patent/pdf/extract-text`

- 源码：`src/routes/upload.rs:1725`
- 响应顶层键：`char_count`、`error`、`full_text`、`method`、`page`、`page_count`、`pages`、`status`、`text`、`total_chars`
- `status` 取值：ok

### §35 GET `/api/patent/image-proxy`

- 源码：`src/routes/patent.rs:1202`
- 读取的查询键：`url`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §36 GET `/api/patent/lookup/:number`

- 源码：`src/routes/patent.rs:478`
- 读取的查询键：`fetch`
- 响应顶层键：`message`、`patent`、`status`
- `status` 取值：error / ok

### §37 POST `/api/patent/lookup-or-fetch`

- 源码：`src/routes/patent.rs:649`
- 读取的请求键：`patent_number`
- 响应顶层键：`message`、`patent`、`source`、`status`
- `status` 取值：error / ok

### §38 POST `/api/patent/resolve-chinese-titles`

- 源码：`src/routes/patent.rs:559`
- 请求体：`ResolveTitlesRequest`（字段见上文类型表）
- 响应顶层键：`status`、`titles`
- `status` 取值：ok

### §39 GET `/api/patent/similar/:id`

- 源码：`src/routes/patent.rs:731`
- 响应顶层键：`error`、`similar`

### §40 GET / POST / PATCH `/api/patent/:id/legal-status`

- **GET** `routes::api_patent_legal_status`（`src/routes/patent.rs:1323`）
  - 响应顶层键：`message`、`result`、`status`
  - `status` 取值：error / ok
- **POST** `routes::api_update_patent_legal_status`（`src/routes/patent.rs:1409`）
  - 请求体：`UpdateLegalStatusRequest`（字段见上文类型表）
  - 响应顶层键：`badge_color`、`legal_status`、`message`、`normalized_status`、`patent_id`、`status`
  - `status` 取值：error / ok
- **PATCH** `routes::api_update_patent_legal_status`（`src/routes/patent.rs:1409`）
  - 请求体：`UpdateLegalStatusRequest`（字段见上文类型表）
  - 响应顶层键：`badge_color`、`legal_status`、`message`、`normalized_status`、`patent_id`、`status`
  - `status` 取值：error / ok

### §41 GET `/api/patent/:id/citation-graph`

- 源码：`src/routes/patent.rs:1451`
- 响应顶层键：`badge_color`、`cited_patents`、`citing_patents`、`country`、`edges`、`family_id`、`family_patents`、`from`、`id`、`label`、`legal_status`、`message`、`nodes`、`normalized_status`、`patent_number`、`relation`、`role`、`status`、`title`、`to`
- `status` 取值：error / ok

### §42 POST `/api/ai/chat`

- 源码：`src/routes/ai.rs:440`
- 请求体：`AiChatRequest`（字段见上文类型表）
- 响应顶层键：`content`、`image_url`、`messages`、`model`、`role`、`temperature`、`text`、`type`

### §44 POST `/api/ai/chat/conclusions`

- 源码：`src/routes/ai.rs:626`
- 读取的请求键：`history`、`patent_id`、`session_key`
- 响应顶层键：`conclusions`、`error`、`status`
- `status` 取值：ok

### §45 GET `/api/auth/google/url`

- 源码：`src/routes/auth.rs:387`
- 响应顶层键：`error`、`redirect_uri`、`url`

### §47 POST `/api/auth/google/exchange`

- 源码：`src/routes/auth.rs:447`
- 响应顶层键：`error`、`expires_in`、`has_refresh_token`、`message`、`status`
- `status` 取值：ok

### §48 GET `/api/auth/google/status`

- 源码：`src/routes/auth.rs:549`
- 响应顶层键：`auth_mode`、`client_id_set`、`client_secret_set`、`has_credentials`、`has_token`、`refresh_token_set`、`token_expired`、`token_expiry`

### §49 GET `/api/auth/gcloud/status`

- 源码：`src/routes/auth.rs:60`
- 响应顶层键：`adc_email`、`adc_file_exists`、`adc_file_path`、`adc_type`、`gcloud_available`、`has_token`、`is_gcloud_mode`、`token_expired`

### §50 POST `/api/auth/gcloud/login`

- 源码：`src/routes/auth.rs:116`
- 响应顶层键：`expires_in`、`message`、`method`、`status`
- `status` 取值：error / ok

### §52 POST `/api/ai/compare`

- 源码：`src/routes/ai.rs:781`
- 读取的请求键：`items`、`patent_id1`、`patent_id2`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §53 POST `/api/ai/claims`

- 源码：`src/routes/ai.rs:924`
- 读取的请求键：`patent_id`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §54 POST `/api/ai/risk`

- 源码：`src/routes/ai.rs:953`
- 读取的请求键：`patent_ids`、`product_description`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §55 POST `/api/ai/compare-matrix`

- 源码：`src/routes/ai.rs:1016`
- 读取的请求键：`items`、`patent_ids`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §56 POST `/api/ai/batch-summarize`

- 源码：`src/routes/ai.rs:1102`
- 读取的请求键：`patent_ids`
- 响应顶层键：`error`、`id`、`patent_number`、`status`、`summaries`、`summary`、`title`
- `status` 取值：ok

### §57 POST `/api/ai/inventiveness-analysis`

- 源码：`src/routes/ai.rs:1263`
- 读取的请求键：`reference_ids`、`references`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §58 POST `/api/ai/office-action-response`

- 源码：`src/routes/oa_core.rs:57`
- 读取的请求键：`depth`、`discuss`、`first_exam_context`、`force`、`historical_constraints`、`my_patent`、`my_patent_id`、`oa_date`、`oa_type`、`office_action`、`references`、`strategy_hint`
- 响应顶层键：`analysis`、`cached`、`error`、`patent_number`、`patent_title`、`status`、`version`
- `status` 取值：ok

### §59 POST `/api/ai/office-action-response/stream`

- 源码：`src/routes/oa_core.rs:355`
- 读取的请求键：`case_meta`、`depth`、`discuss`、`first_exam_context`、`historical_constraints`、`my_patent`、`my_patent_id`、`oa_date`、`oa_type`、`office_action`、`references`、`strategy_hint`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §60 POST `/api/ai/oa-generate-response-letter`

- 源码：`src/routes/oa_core.rs:677`
- 读取的请求键：`analysis`、`analysis_source`、`case_meta`、`current_claims`、`discussion`、`discussion_id`、`historical_constraints`、`my_patent`、`oa_date`、`oa_full_text`、`oa_type`、`office_action`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §61 POST `/api/ai/oa-discuss`

- 源码：`src/routes/oa_core.rs:1023`
- 读取的请求键：`analysis`、`applicant_name`、`discussion`、`discussion_id`、`file_base64`、`file_name`、`file_type`、`message`、`oa_text`、`oa_type`、`office_action`、`patent_number`
- 响应顶层键：`content`、`role`

### §62 POST `/api/ai/oa-fetch-refs`

- 源码：`src/routes/oa_assist.rs:6`
- 读取的请求键：`oa_text`
- 响应顶层键：`abstract`、`context`、`found`、`found_count`、`full_text`、`message`、`pub_number`、`refs`、`source`、`status`、`title`、`total`
- `status` 取值：error / ok

### §63 POST `/api/ai/oa-panel`

- 源码：`src/routes/oa_assist.rs:94`
- 读取的请求键：`my_patent`、`oa_text`、`refs`
- 响应顶层键：`chair`、`error`、`rebuttals`、`roles`、`status`、`synthesis`
- `status` 取值：ok

### §64 POST `/api/ai/oa-history-compare`

- 源码：`src/routes/oa_assist.rs:275`
- 读取的请求键：`current_oa_text`、`current_oa_type`、`patent_id`
- 响应顶层键：`analysis`、`error`、`history_count`、`message`、`status`
- `status` 取值：no_history / ok

### §65 POST `/api/ai/oa-examiner-preview`

- 源码：`src/routes/oa_assist.rs:343`
- 读取的请求键：`my_patent`、`oa_text`、`response_text`
- 响应顶层键：`error`、`preview`、`status`
- `status` 取值：ok

### §66 POST `/api/ai/oa-defense-analysis`

- 源码：`src/routes/oa_assist.rs:390`
- 读取的请求键：`claims`、`my_patent`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §67 POST `/api/ai/oa-strategy-recommend`

- 源码：`src/routes/oa_assist.rs:445`
- 读取的请求键：`my_patent`、`oa_text`、`oa_type`、`refs`
- 响应顶层键：`error`、`status`、`strategies`
- `status` 取值：ok

### §68 POST `/api/ai/oa-quality-score`

- 源码：`src/routes/oa_assist.rs:483`
- 读取的请求键：`my_patent`、`oa_text`、`response_text`
- 响应顶层键：`error`
- `status` 取值：ok

### §69 POST `/api/ai/audit-checks`

- 源码：`src/routes/audit.rs:17`
- 读取的请求键：`ai_output`、`case_id`、`documents`、`golden_output`、`key_terms`、`known_features`、`oa_text`、`patent_id`、`pdf_text`、`response_draft`、`response_text`、`submitted_claim_count`、`submitted_claims`
- 响应顶层键：`actual_sources`、`case_id`、`char_diff_count`、`citations`、`claimed_source`、`concluding_paragraph`、`count`、`covered_features`、`dependent_claim_evaluations`、`distinction_features`、`doc_id`、`entries`、`entry_count`、`error`、`estoppel_check`、`evaluated_claims`、`feature`、`feature_coverage`、`features`、`filing_date`、`filing_info`、`filing_serial`、`found_in_original`、`involved_claims`、`ipc`、`is_correct`、`is_reliable`、`match_position`、`method`、`misattribution_count`、`mismatch_count`、`note`、`paraphrase_text`、`passed`、`publication_number`、`quoted_text`、`redline_count`、`rejected_claims`、`relevance_type`、`response_deadline_months`、`response_deadline_text`、`results`、`risk_warning`、`rule_description`、`rule_id`、`segment_count`、`segments`、`severity`、`similarity`、`skipped`、`status`、`summary`、`term_coverage_diff`、`text_length`、`total_rounds`、`uncovered_features`、`unevaluated_claims`、`violating_text`、`violations`
- `status` 取值：available / ok

### §70 POST `/api/ai/prohibition-check`

- 源码：`src/routes/audit.rs:289`
- 读取的请求键：`response_text`
- 响应顶层键：`count`、`error`、`rule_description`、`rule_id`、`severity`、`status`、`summary`、`violating_text`、`violations`
- `status` 取值：ok

### §71 POST `/api/ai/oa-claim-simulate`

- 源码：`src/routes/oa_assist.rs:644`
- 读取的请求键：`modified_claims`、`oa_text`、`original_claims`、`refs`
- 响应顶层键：`error`、`raw`、`simulation`、`status`
- `status` 取值：ok

### §72 POST `/api/ai/oa-collect-evidence`

- 源码：`src/routes/oa_assist.rs:795`
- 读取的请求键：`my_patent`、`oa_text`、`refs`
- 响应顶层键：`evidence`、`message`、`stats`、`status`、`summary`
- `status` 取值：error / ok

### §73 POST `/api/ai/oa-adversarial`

- 源码：`src/routes/oa_assist.rs:867`
- 读取的请求键：`claims`、`focus`、`goal_threshold`、`my_patent`、`oa_text`、`refs`、`rounds`
- 响应顶层键：`confidence_score`、`converged`、`error`、`final_response`、`goal_achieved`、`goal_threshold`、`rounds_completed`、`status`、`transcript`、`weak_points`
- `status` 取值：ok

### §74 POST `/api/ai/vision-figure-check`

- 源码：`src/routes/oa_assist.rs:923`
- 读取的请求键：`image_base64_urls`、`patent_id`、`spec_text`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §75 POST `/api/ai/oa-feature-matrix`

- 源码：`src/routes/oa_assist.rs:974`
- 读取的请求键：`claims`、`refs`、`spec_text`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §76 GET `/api/oa/history/all`

- 源码：`src/routes/oa_core.rs:1363`
- 响应顶层键：`analyses`、`message`、`status`
- `status` 取值：error / ok

### §77 GET `/api/oa/history/:patent_number`

- 源码：`src/routes/oa_core.rs:1352`
- 响应顶层键：`analyses`、`message`、`status`
- `status` 取值：error / ok

### §78 GET `/api/oa/history/detail/:id`

- 源码：`src/routes/oa_core.rs:1374`
- 响应顶层键：`analysis`、`message`、`status`
- `status` 取值：error / ok

### §79 POST `/api/oa/history/:id/delete`

- 源码：`src/routes/oa_core.rs:1482`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §80 GET `/api/oa/history/:patent_number/diff`

- 源码：`src/routes/oa_diff.rs:21`
- 响应顶层键：`added_refs`、`analyses`、`analysis_diff`、`analysis_full`、`analysis_preview`、`created_at`、`depth`、`depth_changed`、`diffs`、`first_exam_context`、`from_analysis_preview`、`from_claim_changes`、`from_date`、`from_depth`、`from_oa_type`、`from_version`、`history`、`message`、`new_reasons`、`oa_type`、`oa_type_changed`、`overcome_reasons`、`patent_number`、`patent_title`、`ref_diff`、`removed_refs`、`status`、`to_analysis_preview`、`to_claim_changes`、`to_date`、`to_depth`、`to_oa_type`、`to_version`、`total_rounds`、`version`
- `status` 取值：error / ok

### §81 GET `/api/oa/discussions/:patent_number`

- 源码：`src/routes/oa_core.rs:1386`
- 响应顶层键：`discussions`、`message`、`status`
- `status` 取值：error / ok

### §82 GET `/api/oa/discussions/:patent_number/:discussion_id`

- 源码：`src/routes/oa_core.rs:1397`
- 响应顶层键：`discussion`、`message`、`status`
- `status` 取值：error / ok

### §83 POST `/api/oa/discussions/import`

- 源码：`src/routes/oa_core.rs:1419`
- 读取的请求键：`messages`、`oa_text`、`patent_number`
- 响应顶层键：`content`、`discussion_id`、`message`、`ok`、`role`、`status`
- `status` 取值：error / ok

### §84 POST `/api/oa/export-docx`

- 源码：`src/routes/oa_core.rs:1599`
- 读取的请求键：`applicant`、`oa_type`、`patent_number`、`response_text`
- 响应顶层键：`docx`、`error`、`status`
- `status` 取值：ok

### §88 GET `/api/oa/rounds/:patent_id`

- 源码：`src/routes/oa_rounds.rs:15`
- 响应顶层键：`count`、`message`、`patent_id`、`rounds`、`status`
- `status` 取值：error / ok

### §89 GET `/api/oa/:case_id/rounds/diff`

- 源码：`src/routes/oa_rounds.rs:279`
- 响应顶层键：`accepted_features`、`case_id`、`consistency_redlines`、`message`、`new_rejection_grounds`、`newly_introduced_prior_art`、`prompt_constraints_block`、`status`、`summary`、`timeline`、`total_rounds`、`transitions`
- `status` 取值：error / ok

### §90 POST `/api/oa/rounds`

- 源码：`src/routes/oa_rounds.rs:41`
- 读取的请求键：`notes`、`oa_date`、`oa_text`、`patent_id`、`quality_score`、`response_date`、`response_text`、`result`、`result_date`、`round_type`、`strategy_used`
- 响应顶层键：`message`、`round`、`status`
- `status` 取值：error / ok

### §91 PUT `/api/oa/rounds/update/:id`

- 源码：`src/routes/oa_rounds.rs:156`
- 读取的请求键：`notes`、`oa_date`、`oa_text`、`quality_score`、`response_date`、`response_text`、`result`、`result_date`、`strategy_used`
- 响应顶层键：`message`、`round`、`status`
- `status` 取值：error / ok

### §92 GET `/api/oa/timeline/:patent_id`

- 源码：`src/routes/oa_rounds.rs:231`
- 响应顶层键：`analyses`、`has_response`、`has_result`、`patent_id`、`round`、`status`、`summary`、`timeline`
- `status` 取值：ok

### §93 POST `/api/ai/check-amendments`

- 源码：`src/routes/oa_core.rs:1493`
- 读取的请求键：`amended_claims`、`office_action`、`original_claims`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §94 POST `/api/ai/threat-assessment`

- 源码：`src/routes/oa_core.rs:1530`
- 读取的请求键：`my_claims`、`patents`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §95 POST `/api/ai/claim-chart`

- 源码：`src/routes/oa_core.rs:1561`
- 读取的请求键：`my_claims`、`prior_art`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §96 GET `/api/ai/cost`

- 源码：`src/routes/oa_core.rs:1636`
- 响应顶层键：`error`

### §97 GET `/api/ai/cost/records`

- 源码：`src/routes/oa_core.rs:1647`
- 响应顶层键：`count`、`error`、`records`、`status`
- `status` 取值：ok

### §98 POST `/api/ai/cost/record`

- 源码：`src/routes/oa_core.rs:1662`
- 读取的请求键：`duration_ms`、`estimated_cost_cents`、`input_tokens`、`model`、`output_tokens`、`pipeline_run_id`、`provider`、`step`
- 响应顶层键：`error`、`id`、`status`
- `status` 取值：ok

### §99 POST `/api/idea/submit`

- 源码：`src/routes/idea.rs:176`
- 请求体：`IdeaSubmitRequest`（字段见上文类型表）
- 响应顶层键：`id`、`message`、`status`
- `status` 取值：error / ok

### §100 POST `/api/idea/analyze`

- 源码：`src/routes/idea.rs:224`
- 读取的请求键：`id`
- 响应顶层键：`abstract`、`idea`、`link`、`message`、`patent_number`、`snippet`、`source`、`status`、`title`
- `status` 取值：error / ok

### §101 POST `/api/idea/pipeline`

- 源码：`src/routes/idea.rs:1019`
- 读取的请求键：`id`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §102 POST `/api/ideas/batch-compare`

- 源码：`src/routes/idea_reports.rs:599`
- 读取的请求键：`idea_ids`
- 响应顶层键：`description`、`id`、`ideas`、`message`、`novelty_score`、`similarity_matrix`、`status`、`title`
- `status` 取值：error / ok

### §103 GET `/api/idea/list`

- 源码：`src/routes/idea.rs:407`
- 响应顶层键：`ideas`、`message`、`status`
- `status` 取值：error / ok

### §104 GET `/api/idea/:id`

- 源码：`src/routes/idea.rs:375`
- 响应顶层键：`idea`、`message`、`status`
- `status` 取值：error / ok

### §105 POST `/api/idea/:id/delete`

- 源码：`src/routes/idea.rs:417`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §107 POST `/api/idea/:id/resume`

- 源码：`src/routes/idea.rs:1181`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §108 GET / POST `/api/idea/:id/research-state`

- **GET** `routes::api_idea_research_state`（`src/routes/idea_ops.rs:11`）
  - 响应顶层键：`message`、`research_state`、`source`、`status`
  - `status` 取值：error / ok
- **POST** `routes::api_idea_research_state_update`（`src/routes/idea_ops.rs:30`）
  - 请求体：`ResearchStateUpdateRequest`（字段见上文类型表）
  - 响应顶层键：`message`、`research_state`、`status`
  - `status` 取值：error / ok

### §109 POST `/api/idea/:id/redirect`

- 源码：`src/routes/idea_ops.rs:51`
- 请求体：`RedirectRequest`（字段见上文类型表）
- 响应顶层键：`message`、`reason`、`restart_from`、`status`
- `status` 取值：error / ok

### §110 GET `/api/idea/:id/report`

- 源码：`src/routes/idea_reports.rs:8`
- 读取的查询键：`type`、`version_id`
- 响应顶层键：`message`、`patent_results`、`report`、`report_type`、`status`、`web_results`
- `status` 取值：error / ok

### §112 GET `/api/ai/models`

- 源码：`src/routes/idea_ops.rs:906`
- 读取的查询键：`provider`
- 响应顶层键：`base_url`、`configured`、`count`、`error`、`id`、`models`、`name`、`provider`、`providers`、`status`、`total_count`
- `status` 取值：error / not_configured / not_found / ok / timeout

### §113 POST `/api/idea/:id/chat`

- 源码：`src/routes/idea.rs:554`
- 请求体：`IdeaChatRequest`（字段见上文类型表）
- 响应顶层键：`content`、`error`、`image_url`、`message`、`messages`、`model`、`role`、`status`、`temperature`、`text`、`type`
- `status` 取值：ok

### §114 GET `/api/idea/:id/evidence`

- 源码：`src/routes/idea.rs:446`
- 响应顶层键：`count`、`evidence`、`message`、`status`
- `status` 取值：error / ok

### §115 GET `/api/idea/:id/kernel-report`

- 源码：`src/routes/idea.rs:463`
- 响应顶层键：`current_step`、`kernel_report`、`message`、`status`
- `status` 取值：error / no_kernel_report / no_snapshot / ok

### §116 GET `/api/idea/:id/messages`

- 源码：`src/routes/idea.rs:993`
- 响应顶层键：`content`、`created_at`、`error`、`id`、`messages`、`role`

### §117 GET `/api/idea/:id/token-usage`

- 源码：`src/routes/idea.rs:982`
- 响应顶层键：`message`、`status`
- `status` 取值：error

### §118 GET `/api/idea/:id/chat/conclusions`

- 源码：`src/routes/idea_reports.rs:474`
- 响应顶层键：`conclusions`、`error`、`status`
- `status` 取值：ok

### §119 GET `/api/idea/:id/chat/conversation`

- 源码：`src/routes/idea_reports.rs:453`
- 响应顶层键：`content`、`created_at`、`error`、`idea`、`messages`、`role`、`status`
- `status` 取值：ok

### §120 POST `/api/idea/:id/summarize`

- 源码：`src/routes/idea_reports.rs:542`
- 响应顶层键：`error`、`status`、`summary`
- `status` 取值：ok

### §121 GET / POST `/api/ideas/:id/feature-cards`

- **GET** `routes::api_get_feature_cards`（`src/routes/feature_cards.rs:16`）
  - 响应顶层键：`cards`、`message`、`status`
  - `status` 取值：error / ok
- **POST** `routes::api_create_feature_card`（`src/routes/feature_cards.rs:50`）
  - 请求体：`CreateFeatureCardRequest`（字段见上文类型表）
  - 响应顶层键：`card`、`message`、`status`
  - `status` 取值：error / ok

### §122 POST `/api/ideas/:id/feature-cards/generate`

- 源码：`src/routes/feature_cards.rs:585`
- 响应顶层键：`cad_prompts`、`cards`、`message`、`source`、`status`
- `status` 取值：error / ok

### §124 POST `/api/ideas/:id/control-prototype`

- 源码：`src/routes/idea_reports.rs:1084`
- 响应顶层键：`message`、`status`
- `status` 取值：error

### §125 POST `/api/ideas/:id/fto-scan`

- 源码：`src/routes/idea_reports.rs:1124`
- 响应顶层键：`message`、`status`
- `status` 取值：error

### §129 GET `/api/feature-cards/diff`

- 源码：`src/routes/feature_cards.rs:132`
- 响应顶层键：`card_a`、`card_b`、`diff`、`diff_type`、`fallback_used`、`message`、`novelty_reason`、`novelty_significance`、`semantically_equivalent`、`status`
- `status` 取值：error / ok

### §130 PUT `/api/ideas/:id/claims`

- 源码：`src/routes/idea_ops.rs:261`
- 请求体：`ReplaceClaimsRequest`（字段见上文类型表）
- 响应顶层键：`claim_number`、`claim_type`、`claims`、`content`、`count`、`created_at`、`id`、`idea_id`、`message`、`parent_claim_id`、`status`
- `status` 取值：error / ok

### §131 POST `/api/ideas/:id/claims/validate`

- 源码：`src/routes/idea_ops.rs:617`
- 响应顶层键：`message`、`status`
- `status` 取值：error

### §132 GET / POST `/api/idea/:id/claim-tree`

- **GET** `routes::api_idea_claim_tree`（`src/routes/idea_ops.rs:164`）
  - 响应顶层键：`claim_number`、`claim_type`、`claims`、`content`、`created_at`、`description`、`id`、`message`、`novelty_flag`、`parent_claim_id`、`status`
  - `status` 取值：error / ok
- **POST** `routes::api_idea_claim_node_upsert`（`src/routes/idea_ops.rs:457`）
  - 请求体：`UpsertClaimNodeRequest`（字段见上文类型表）
  - 响应顶层键：`claim`、`message`、`status`
  - `status` 取值：error / ok

### §133 DELETE `/api/idea/:id/claim-tree/:claim_id`

- 源码：`src/routes/idea_ops.rs:572`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §134 POST `/api/idea/:id/iterate`

- 源码：`src/routes/idea_reports.rs:699`
- 响应顶层键：`iteration`、`message`、`status`
- `status` 取值：error / ok

### §135 GET `/api/idea/:id/versions`

- 源码：`src/routes/idea_ops.rs:722`
- 响应顶层键：`branch_id`、`created_at`、`current_step`、`id`、`message`、`status`、`version_number`、`versions`
- `status` 取值：error / ok

### §136 GET `/api/idea/:id/branches`

- 源码：`src/routes/idea_ops.rs:747`
- 响应顶层键：`branches`、`message`、`status`
- `status` 取值：error / ok

### §137 GET `/api/idea/:id/findings`

- 源码：`src/routes/idea_ops.rs:758`
- 响应顶层键：`findings`、`message`、`status`
- `status` 取值：error / ok

### §138 GET / POST `/api/idea/:id/memory`

- **GET** `routes::api_idea_memory`（`src/routes/idea_ops.rs:769`）
  - 响应顶层键：`by_type`、`counts`、`entries`、`message`、`status`、`total`
  - `status` 取值：error / ok
- **POST** `routes::api_idea_memory_add`（`src/routes/idea_ops.rs:790`）
  - 读取的请求键：`concept_name`、`concept_type`、`confidence`、`content`
  - 响应顶层键：`id`、`message`、`status`
  - `status` 取值：error / ok

### §139 DELETE `/api/idea/:id/memory/:entry_id`

- 源码：`src/routes/idea_ops.rs:848`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §140 GET `/api/ipc/tree`

- 源码：`src/routes/ipc.rs:22`
- 响应顶层键：`classes`、`code`、`count`、`message`、`name`、`sections`、`status`、`subclasses`、`total_codes`
- `status` 取值：error / ok

### §141 GET `/api/ipc/:code/patents`

- 源码：`src/routes/ipc.rs:122`
- 响应顶层键：`code`、`count`、`message`、`patents`、`status`
- `status` 取值：error / ok

### §142 POST `/api/patents/import`

- 源码：`src/routes/settings.rs:857`
- 请求体：`ImportRequest`（字段见上文类型表）
- 响应顶层键：`chunks`、`imported`、`message`、`status`
- `status` 取值：error / ok

### §144 POST `/api/backup/check`

- 源码：`src/routes/settings.rs:1630`
- 读取的请求键：`path`
- 响应顶层键：`message`、`schema_version`、`size_bytes`、`status`、`valid_sqlite`
- `status` 取值：error / ok

### §145 GET / POST `/api/collections`

- **GET** `routes::api_list_collections`（`src/routes/collections.rs:28`）
  - 响应顶层键：`collections`、`created_at`、`description`、`error`、`id`、`name`、`patent_count`
- **POST** `routes::api_create_collection`（`src/routes/collections.rs:8`）
  - 读取的请求键：`description`、`name`
  - 响应顶层键：`error`、`id`、`name`、`status`
  - `status` 取值：ok

### §146 DELETE `/api/collections/:id`

- 源码：`src/routes/collections.rs:49`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §147 GET `/api/collections/:id/patents`

- 源码：`src/routes/collections.rs:92`
- 响应顶层键：`error`、`patents`

### §148 POST `/api/collections/:id/add`

- 源码：`src/routes/collections.rs:59`
- 读取的请求键：`patent_id`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §149 DELETE `/api/collections/:id/remove/:patent_id`

- 源码：`src/routes/collections.rs:82`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §150 GET / POST `/api/patents/:id/tags`

- **GET** `routes::api_get_patent_tags`（`src/routes/collections.rs:128`）
  - 响应顶层键：`error`、`tags`
- **POST** `routes::api_add_tag`（`src/routes/collections.rs:102`）
  - 读取的请求键：`tag`
  - 响应顶层键：`error`、`status`
  - `status` 取值：ok

### §151 DELETE `/api/patents/:id/tags/:tag`

- 源码：`src/routes/collections.rs:118`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §152 GET `/api/patents/:id/collections`

- 源码：`src/routes/collections.rs:151`
- 响应顶层键：`collection_ids`、`error`

### §153 GET `/api/tags`

- 源码：`src/routes/collections.rs:138`
- 响应顶层键：`count`、`error`、`tag`、`tags`

### §154 POST `/api/upload/compare`

- 源码：`src/routes/upload.rs:340`
- 响应顶层键：`analysis`、`content_length`、`error`、`file_type`、`success`

### §155 POST `/api/upload/extract`

- 源码：`src/routes/upload.rs:518`
- 响应顶层键：`char_length`、`file_type`、`length`、`text`、`truncated`

### §156 POST `/api/upload/extract/stream`

- 源码：`src/routes/upload.rs:678`
- 响应顶层键：`char_length`、`file_type`、`length`、`method`、`text`、`total_pages`、`truncated`

### §157 POST `/api/upload/pdf-store`

- 源码：`src/routes/upload.rs:275`
- 响应顶层键：`filename`、`message`、`size`、`status`、`url`
- `status` 取值：error / ok

### §158 GET `/api/patent/:id/numeral-check`

- 源码：`src/routes/patent.rs:414`
- 响应顶层键：`issue_count`、`issues`、`message`、`parts`、`passed`、`patent_id`、`patent_number`、`status`、`summary`、`total_numerals`
- `status` 取值：error / ok

### §159 POST `/api/ai/numeral-consistency-check`

- 源码：`src/routes/patent.rs:441`
- 读取的请求键：`claims_text`、`description_text`、`drawing_parts`
- 响应顶层键：`issue_count`、`issues`、`message`、`parts`、`passed`、`status`、`summary`、`total_numerals`
- `status` 取值：error / ok

## 已知缺口

- 1 条任意 JSON 端点**未抽取到请求键**（处理器可能完全不用请求体字段，或经共享 helper/嵌套结构读取）。
- 响应只列顶层键：嵌套对象的内部键、数组元素结构、以及动态拼接的键未展开。
- 不覆盖：鉴权要求、速率限制、错误码语义。这些属于运行时行为，见 `docs/API.md` 与对应源码。
