# API 契约（自动生成）

> 本文件由 `scripts/gen_api_contracts.js` 从 `src/common.rs` 的路由声明与处理函数源码生成，**不要手改**。
> `tests/api_contracts_up_to_date.rs` 会用同一脚本的 `--check` 模式比对，代码改了没重跑就红。
> 叙述性说明、示例与业务语义仍写在 [`docs/API.md`](API.md)。

共 **169** 条 `/api` 路由（另有 16 条页面/静态路由，见 [API-ROUTES.md](API-ROUTES.md)）。

## 怎么读

- **路径参数**：取自路由模板里的 `:name`；处理函数用 `Path<T>` 接。
- **请求体 / 查询**：写出类型名；类型能在 `src/` 里找到 `struct` 定义时，下文按字段列出（含 serde 重命名与是否可省略）。
- **任意 JSON**：端点的类型是 `serde_json::Value`，字段没有类型约束。本文件从函数体里**静态抽取**它实际读取的键与返回的顶层键；
  经共享 helper 读取的键可能抽不到——抽不到会写明「未抽取到」，不拿「没写」冒充「没有」。
- **响应**：能确定 `Json<T>` 的写类型；`Response`/`impl IntoResponse` 这类动态响应只标注形态，状态码与正文见源码行。

## 概览

| 维度 | 数量 |
| --- | --- |
| `/api` 路由 | 169 |
| 请求体是具体结构体 | 46 |
| 请求体是任意 JSON | 46 |
| 其中能从函数体抽到请求键 | 45 |
| 响应含可识别 JSON 顶层键 | 145 |
| 引用的请求/查询类型已解析到 struct | 45/45 |
| 处理函数未找到 | 0 |

## 路由契约总表

| # | 方法 | 路径 | 路径参数 | 请求体 | 查询 | 响应 | 处理函数 | 源码 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | GET | `/api/healthz` | — | — | — | JSON（§1） | `crate::app::api_healthz` | `src/app.rs:174` |
| 2 | POST | `/api/auth/unlock` | — | `AuthUnlockRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `crate::app::api_auth_unlock` | `src/app.rs:183` |
| 3 | GET | `/api/settings/health` | — | — | — | JSON（未抽到顶层键） | `crate::cards::settings_vault::api_settings_health` | `src/cards/settings_vault/routes.rs:25` |
| 4 | GET / POST | `/api/settings/config` | — | `SettingsConfigUpdateReq` | — | JSON（§4） | `crate::cards::settings_vault::api_settings_get_config`<br>`crate::cards::settings_vault::api_settings_save_config` | `src/cards/settings_vault/routes.rs:30`<br>`src/cards/settings_vault/routes.rs:35` |
| 5 | POST | `/api/settings/probe-provider` | — | `ProbeProviderReq` | — | JSON（未抽到顶层键） | `crate::cards::settings_vault::api_settings_probe_provider` | `src/cards/settings_vault/routes.rs:52` |
| 6 | POST | `/api/vault/import-pack` | — | `VaultImportPackReq` | — | JSON（§6） | `crate::cards::settings_vault::api_vault_import_pack` | `src/cards/settings_vault/routes.rs:60` |
| 7 | GET / DELETE / PATCH | `/api/vault/docs` | — | `VaultPatchDocReq` | — | JSON（§7） | `crate::cards::settings_vault::api_vault_list_docs`<br>`crate::cards::settings_vault::api_vault_delete_doc`<br>`crate::cards::settings_vault::api_vault_patch_doc` | `src/cards/settings_vault/routes.rs:77`<br>`src/cards/settings_vault/routes.rs:102`<br>`src/cards/settings_vault/routes.rs:85` |
| 8 | POST | `/api/omni-search/query` | — | `OmniSearchQueryReq` | — | JSON（§8） | `crate::cards::omni_search::api_omni_search_query` | `src/cards/omni_search/routes.rs:97` |
| 9 | POST | `/api/omni-search/chunks` | — | `OmniSearchChunksReq` | — | JSON（§9） | `crate::cards::omni_search::api_omni_search_chunks` | `src/cards/omni_search/routes.rs:155` |
| 10 | GET | `/api/omni-search/doc/{id}` | `id` | — | — | JSON（§10） | `crate::cards::omni_search::api_omni_search_doc` | `src/cards/omni_search/routes.rs:195` |
| 11 | POST | `/api/omni-search/export` | — | `OmniSearchExportReq` | — | HTTP 响应（动态，状态码/正文见源码） | `crate::cards::omni_search::api_omni_search_export` | `src/cards/omni_search/routes.rs:224` |
| 12 | POST | `/api/idea-studio/session` | — | `StudioCreateSessionReq` | — | JSON（§12） | `crate::cards::idea_studio::api_studio_create_session` | `src/cards/idea_studio/routes.rs:135` |
| 13 | POST | `/api/idea-studio/session/{id}/turn` | `id` | `StudioTurnReq` | — | JSON（§13） | `crate::cards::idea_studio::api_studio_turn` | `src/cards/idea_studio/routes.rs:154` |
| 14 | POST | `/api/idea-studio/run/start` | — | `StudioRunStartReq` | — | JSON（§14） | `crate::cards::idea_studio::api_studio_run_start` | `src/cards/idea_studio/routes.rs:181` |
| 15 | POST | `/api/idea-studio/run/{id}/fork` | `id` | `StudioRunForkReq` | — | JSON（§15） | `crate::cards::idea_studio::api_studio_run_fork` | `src/cards/idea_studio/routes.rs:204` |
| 16 | POST | `/api/idea-studio/run/{id}/checkpoint` | `id` | `StudioCheckpointReq` | — | JSON（§16） | `crate::cards::idea_studio::api_studio_run_checkpoint` | `src/cards/idea_studio/routes.rs:234` |
| 17 | POST | `/api/idea-studio/run/{id}/experiment` | `id` | `StudioExperimentReq` | — | JSON（§17） | `crate::cards::idea_studio::api_studio_run_experiment` | `src/cards/idea_studio/routes.rs:258` |
| 18 | GET | `/api/idea-studio/run/{id}/events` | `id` | — | — | SSE 事件流（text/event-stream） | `crate::cards::idea_studio::api_studio_run_events` | `src/cards/idea_studio/routes.rs:294` |
| 19 | POST | `/api/deep-dissect/ingest` | — | `DissectIngestReq` | — | JSON（§19） | `crate::cards::deep_dissect::api_dissect_ingest` | `src/cards/deep_dissect/routes.rs:135` |
| 20 | POST | `/api/deep-dissect/analyze` | — | `DissectAnalyzeReq` | — | JSON（§20） | `crate::cards::deep_dissect::api_dissect_analyze` | `src/cards/deep_dissect/routes.rs:174` |
| 21 | POST | `/api/deep-dissect/ask` | — | `DissectAskReq` | — | JSON（§21） | `crate::cards::deep_dissect::api_dissect_ask` | `src/cards/deep_dissect/routes.rs:214` |
| 22 | POST | `/api/scheme-matrix/compare` | — | `SchemeMatrixCompareReq` | — | JSON（§22） | `crate::cards::scheme_matrix::api_matrix_compare` | `src/cards/scheme_matrix/routes.rs:45` |
| 23 | GET | `/api/scheme-matrix/{id}` | `id` | — | — | JSON（§23） | `crate::cards::scheme_matrix::api_matrix_get` | `src/cards/scheme_matrix/routes.rs:80` |
| 24 | POST | `/api/scheme-matrix/{id}/export` | `id` | `SchemeMatrixExportReq` | — | HTTP 响应（动态，状态码/正文见源码） | `crate::cards::scheme_matrix::api_matrix_export` | `src/cards/scheme_matrix/routes.rs:113` |
| 25 | POST | `/api/rebuttal-hub/case` | — | `RebuttalCreateCaseReq` | — | JSON（§25） | `crate::cards::rebuttal_hub::api_rebuttal_create_case` | `src/cards/rebuttal_hub/routes.rs:106` |
| 26 | POST | `/api/rebuttal-hub/case/{id}/round` | `id` | `RebuttalAddRoundReq` | — | JSON（§26） | `crate::cards::rebuttal_hub::api_rebuttal_add_round` | `src/cards/rebuttal_hub/routes.rs:143` |
| 27 | POST | `/api/rebuttal-hub/case/{id}/mock-review` | `id` | `RebuttalMockReviewReq` | — | JSON（§27） | `crate::cards::rebuttal_hub::api_rebuttal_mock_review` | `src/cards/rebuttal_hub/routes.rs:169` |
| 28 | POST | `/api/rebuttal-hub/case/{id}/draft` | `id` | `RebuttalDraftReq` | — | JSON（§28） | `crate::cards::rebuttal_hub::api_rebuttal_draft` | `src/cards/rebuttal_hub/routes.rs:191` |
| 29 | POST | `/api/rebuttal-hub/case/{id}/verify` | `id` | `RebuttalVerifyReq` | — | JSON（§29） | `crate::cards::rebuttal_hub::api_rebuttal_verify` | `src/cards/rebuttal_hub/routes.rs:209` |
| 30 | POST | `/api/rebuttal-hub/case/{id}/export` | `id` | `RebuttalExportReq` | — | HTTP 响应（动态，状态码/正文见源码） | `crate::cards::rebuttal_hub::api_rebuttal_export` | `src/cards/rebuttal_hub/routes.rs:236` |
| 31 | POST | `/api/deliverable-forge/priority-check` | — | `ForgePriorityCheckReq` | — | JSON（§31） | `crate::cards::deliverable_forge::api_forge_priority_check` | `src/cards/deliverable_forge/routes.rs:93` |
| 32 | POST | `/api/deliverable-forge/compile` | — | `ForgeCompileReq` | — | JSON（§32） | `crate::cards::deliverable_forge::api_forge_compile` | `src/cards/deliverable_forge/routes.rs:121` |
| 33 | POST | `/api/deliverable-forge/export` | — | `ForgeExportReq` | — | HTTP 响应（动态，状态码/正文见源码） | `crate::cards::deliverable_forge::api_forge_export` | `src/cards/deliverable_forge/routes.rs:162` |
| 34 | POST | `/api/multimodal/upload` | — | — | — | JSON（§34） | `crate::cards::multimodal_api::api_multimodal_upload` | `src/cards/multimodal_api.rs:20` |
| 35 | GET | `/api/multimodal/task/{id}/events` | `id` | — | — | SSE 事件流（text/event-stream） | `crate::cards::multimodal_api::api_multimodal_task_events` | `src/cards/multimodal_api.rs:95` |
| 36 | GET | `/api/cad/status` | — | — | — | JSON（未抽到顶层键） | `routes::api_cad_status` | `src/routes/cad.rs:41` |
| 37 | POST | `/api/cad/start` | — | — | — | JSON（未抽到顶层键） | `routes::api_cad_start` | `src/routes/cad.rs:45` |
| 38 | POST | `/api/cad/draw` | — | `CadDrawRequest` | — | JSON CadDrawResponse | `routes::api_cad_draw` | `src/routes/cad.rs:49` |
| 39 | GET | `/api/cad/artifacts` | — | — | `CadHistoryQuery` | JSON（§39） | `routes::api_cad_artifacts` | `src/routes/cad.rs:165` |
| 40 | GET | `/api/cad/artifacts/{id}/preview` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_cad_preview` | `src/routes/cad.rs:189` |
| 41 | GET | `/api/cad/artifacts/{id}/download/{format}` | `id`, `format` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_cad_download` | `src/routes/cad.rs:197` |
| 42 | GET | `/api/version` | — | — | — | JSON（§42） | `routes::api_get_version` | `src/routes/settings.rs:57` |
| 43 | GET | `/api/oa/deadline-table` | — | — | — | JSON（§43） | `crate::oa_deadline::deadline_table` | `src/oa_deadline.rs:158` |
| 44 | GET | `/api/oa/compute-deadline` | — | — | — | JSON（§44） | `crate::oa_deadline::compute_deadline_api` | `src/oa_deadline.rs:176` |
| 45 | GET | `/api/settings` | — | — | — | JSON（§45） | `routes::api_get_settings` | `src/routes/settings.rs:78` |
| 46 | POST | `/api/settings/cad` | — | `CadSettingsRequest` | — | JSON（§46） | `routes::api_save_cad_settings` | `src/routes/settings.rs:198` |
| 47 | POST | `/api/settings/search-agent` | — | 任意 JSON（§47） | — | JSON（§47） | `routes::api_save_search_agent_settings` | `src/routes/settings.rs:169` |
| 48 | POST | `/api/settings/serpapi` | — | 任意 JSON（§48） | — | JSON（§48） | `routes::api_save_serpapi` | `src/routes/settings.rs:472` |
| 49 | POST | `/api/settings/ai` | — | 任意 JSON（§49） | — | JSON（§49） | `routes::api_save_ai` | `src/routes/settings.rs:534` |
| 50 | GET | `/api/settings/serpapi/balance` | — | — | — | JSON（§50） | `routes::api_serpapi_balance` | `src/routes/settings.rs:744` |
| 51 | GET | `/api/chat/{session_key}` | `session_key` | — | `ChatPagination` | JSON（§51） | `routes::api_chat_get_messages` | `src/routes/chat.rs:21` |
| 52 | POST | `/api/chat/{session_key}/save` | `session_key` | 任意 JSON（§52） | — | JSON（§52） | `routes::api_chat_save_message` | `src/routes/chat.rs:53` |
| 53 | POST | `/api/chat/{session_key}/delete` | `session_key` | — | — | JSON（§53） | `routes::api_chat_delete_messages` | `src/routes/chat.rs:90` |
| 54 | POST | `/api/search` | — | `SearchRequest` | — | JSON SearchResult | `routes::api_search` | `src/routes/search.rs:412` |
| 55 | POST | `/api/search/stats` | — | `SearchRequest` | — | JSON（§55） | `routes::api_search_stats` | `src/routes/search.rs:1466` |
| 56 | POST | `/api/search/export` | — | `SearchRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_export_csv` | `src/routes/search.rs:1530` |
| 57 | POST | `/api/search/export/xlsx` | — | `SearchRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_export_xlsx` | `src/routes/search.rs:1581` |
| 58 | POST | `/api/search/online` | — | `SearchRequest` | — | JSON（§58） | `routes::api_search_online` | `src/routes/search.rs:671` |
| 59 | POST | `/api/search/vector` | — | 任意 JSON（§59） | — | JSON（§59） | `routes::api_search_vector` | `src/routes/search.rs:1305` |
| 60 | POST | `/api/search/analyze` | — | 任意 JSON（§60） | — | JSON（§60） | `routes::api_ai_analyze_results` | `src/routes/ai.rs:811` |
| 61 | POST | `/api/patent/fetch` | — | `FetchPatentRequest` | — | JSON（§61） | `routes::api_fetch_patent` | `src/routes/patent.rs:14` |
| 62 | GET | `/api/patent/enrich/{id}` | `id` | — | — | JSON（§62） | `routes::api_enrich_patent` | `src/routes/patent.rs:35` |
| 63 | GET | `/api/patent/enrich-free/{id}` | `id` | — | — | JSON（§63） | `routes::api_enrich_patent_free` | `src/routes/patent.rs:247` |
| 64 | GET | `/api/patent/pdf/{id}` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_patent_pdf` | `src/routes/patent.rs:679` |
| 65 | POST | `/api/patent/pdf/extract-text` | — | — | — | JSON（§65） | `routes::api_patent_pdf_extract_text` | `src/routes/upload.rs:1139` |
| 66 | GET | `/api/patent/image-proxy` | — | — | 任意键值对（§66） | HTTP 响应（动态，状态码/正文见源码） | `routes::api_patent_image_proxy` | `src/routes/patent.rs:1002` |
| 67 | GET | `/api/patent/lookup/{number}` | `number` | — | 任意键值对（§67） | JSON（§67） | `routes::api_patent_lookup` | `src/routes/patent.rs:282` |
| 68 | POST | `/api/patent/lookup-or-fetch` | — | 任意 JSON（§68） | — | JSON（§68） | `routes::api_patent_lookup_and_fetch` | `src/routes/patent.rs:453` |
| 69 | POST | `/api/patent/resolve-chinese-titles` | — | `ResolveTitlesRequest` | — | JSON（§69） | `routes::api_resolve_chinese_titles` | `src/routes/patent.rs:363` |
| 70 | GET | `/api/patent/similar/{id}` | `id` | — | — | JSON（§70） | `routes::api_recommend_similar` | `src/routes/patent.rs:535` |
| 71 | GET | `/api/patent/{id}/legal-status` | `id` | — | — | JSON（§71） | `routes::api_patent_legal_status` | `src/routes/patent.rs:1122` |
| 72 | POST | `/api/ai/chat` | — | `AiChatRequest` | — | JSON AiResponse | `routes::api_ai_chat` | `src/routes/ai.rs:410` |
| 73 | POST | `/api/ai/chat/stream` | — | `AiChatRequest` | — | SSE 事件流（text/event-stream） | `routes::api_ai_chat_stream` | `src/routes/ai.rs:357` |
| 74 | POST | `/api/ai/chat/conclusions` | — | 任意 JSON（§74） | — | JSON（§74） | `routes::api_ai_chat_conclusions` | `src/routes/ai.rs:591` |
| 75 | GET | `/api/auth/google/url` | — | — | — | JSON（§75） | `routes::api_google_auth_url` | `src/routes/auth.rs:387` |
| 76 | GET | `/api/auth/google/callback` | — | — | — | (axum::http::StatusCode, &'static str) | `routes::api_google_callback` | `src/routes/auth.rs:406` |
| 77 | POST | `/api/auth/google/exchange` | — | — | — | JSON（§77） | `routes::api_google_exchange_handler` | `src/routes/auth.rs:447` |
| 78 | GET | `/api/auth/google/status` | — | — | — | JSON（§78） | `routes::api_google_oauth_status` | `src/routes/auth.rs:549` |
| 79 | GET | `/api/auth/gcloud/status` | — | — | — | JSON（§79） | `routes::api_gcloud_status` | `src/routes/auth.rs:60` |
| 80 | POST | `/api/auth/gcloud/login` | — | — | — | JSON（§80） | `routes::api_gcloud_login` | `src/routes/auth.rs:116` |
| 81 | POST | `/api/ai/summarize` | — | `FetchPatentRequest` | — | JSON AiResponse | `routes::api_ai_summarize` | `src/routes/ai.rs:681` |
| 82 | POST | `/api/ai/compare` | — | 任意 JSON（§82） | — | JSON AiResponse | `routes::api_ai_compare` | `src/routes/ai.rs:745` |
| 83 | POST | `/api/ai/claims` | — | 任意 JSON（§83） | — | JSON（§83） | `routes::api_ai_claims_analysis` | `src/routes/ai.rs:888` |
| 84 | POST | `/api/ai/risk` | — | 任意 JSON（§84） | — | JSON（§84） | `routes::api_ai_risk_assessment` | `src/routes/ai.rs:917` |
| 85 | POST | `/api/ai/compare-matrix` | — | 任意 JSON（§85） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_compare_matrix` | `src/routes/ai.rs:980` |
| 86 | POST | `/api/ai/batch-summarize` | — | 任意 JSON（§86） | — | JSON（§86） | `routes::api_ai_batch_summarize` | `src/routes/ai.rs:1066` |
| 87 | POST | `/api/ai/inventiveness-analysis` | — | 任意 JSON（§87） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_inventiveness_analysis` | `src/routes/ai.rs:1220` |
| 88 | POST | `/api/ai/office-action-response` | — | 任意 JSON（§88） | — | JSON（§88） | `routes::api_ai_office_action_response` | `src/routes/ai.rs:1280` |
| 89 | POST | `/api/ai/office-action-response/stream` | — | 任意 JSON（§89） | — | SSE 事件流（text/event-stream） | `routes::api_ai_office_action_response_stream` | `src/routes/ai.rs:1571` |
| 90 | POST | `/api/ai/oa-generate-response-letter` | — | 任意 JSON（§90） | — | SSE 事件流（text/event-stream） | `routes::api_ai_oa_generate_response_letter` | `src/routes/ai.rs:1894` |
| 91 | POST | `/api/ai/oa-discuss` | — | 任意 JSON（§91） | — | SSE 事件流（text/event-stream） | `routes::api_ai_oa_discuss` | `src/routes/ai.rs:2197` |
| 92 | POST | `/api/ai/oa-fetch-refs` | — | 任意 JSON（§92） | — | JSON（§92） | `routes::api_ai_oa_fetch_refs` | `src/routes/ai.rs:2878` |
| 93 | POST | `/api/ai/oa-panel` | — | 任意 JSON（§93） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_panel` | `src/routes/ai.rs:2966` |
| 94 | POST | `/api/ai/oa-history-compare` | — | 任意 JSON（§94） | — | JSON（§94） | `routes::api_ai_oa_history_compare` | `src/routes/ai.rs:3147` |
| 95 | POST | `/api/ai/oa-examiner-preview` | — | 任意 JSON（§95） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_examiner_preview` | `src/routes/ai.rs:3215` |
| 96 | POST | `/api/ai/oa-defense-analysis` | — | 任意 JSON（§96） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_defense_analysis` | `src/routes/ai.rs:3262` |
| 97 | POST | `/api/ai/oa-strategy-recommend` | — | 任意 JSON（§97） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_strategy_recommend` | `src/routes/ai.rs:3317` |
| 98 | POST | `/api/ai/oa-quality-score` | — | 任意 JSON（§98） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_quality_score` | `src/routes/ai.rs:3355` |
| 99 | POST | `/api/ai/audit-checks` | — | 任意 JSON（§99） | — | JSON（§99） | `routes::api_ai_audit_checks` | `src/routes/audit.rs:16` |
| 100 | POST | `/api/ai/prohibition-check` | — | 任意 JSON（§100） | — | JSON（§100） | `routes::api_ai_prohibition_check` | `src/routes/audit.rs:257` |
| 101 | POST | `/api/ai/oa-claim-simulate` | — | 任意 JSON（§101） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_claim_simulate` | `src/routes/ai.rs:3516` |
| 102 | POST | `/api/ai/oa-collect-evidence` | — | 任意 JSON（§102） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_collect_evidence` | `src/routes/ai.rs:3667` |
| 103 | POST | `/api/ai/oa-adversarial` | — | 任意 JSON（§103） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_oa_adversarial` | `src/routes/ai.rs:3739` |
| 104 | GET | `/api/oa/history/all` | — | — | — | JSON（§104） | `routes::api_oa_history_all` | `src/routes/ai.rs:2537` |
| 105 | GET | `/api/oa/history/{patent_number}` | `patent_number` | — | — | JSON（§105） | `routes::api_oa_history` | `src/routes/ai.rs:2526` |
| 106 | GET | `/api/oa/history/detail/{id}` | `id` | — | — | JSON（§106） | `routes::api_oa_history_detail` | `src/routes/ai.rs:2548` |
| 107 | POST | `/api/oa/history/{id}/delete` | `id` | — | — | JSON（§107） | `routes::api_oa_history_delete` | `src/routes/ai.rs:2656` |
| 108 | GET | `/api/oa/history/{patent_number}/diff` | `patent_number` | — | — | JSON（§108） | `routes::api_oa_history_diff` | `src/routes/oa_diff.rs:21` |
| 109 | GET | `/api/oa/discussions/{patent_number}` | `patent_number` | — | — | JSON（§109） | `routes::api_oa_discussion_list` | `src/routes/ai.rs:2560` |
| 110 | GET | `/api/oa/discussions/{patent_number}/{discussion_id}` | `patent_number`, `discussion_id` | — | — | JSON（§110） | `routes::api_oa_discussion_get` | `src/routes/ai.rs:2571` |
| 111 | POST | `/api/oa/discussions/import` | — | 任意 JSON（§111） | — | JSON（§111） | `routes::api_oa_discussion_import` | `src/routes/ai.rs:2593` |
| 112 | POST | `/api/oa/export-docx` | — | 任意 JSON（§112） | — | JSON（§112） | `routes::api_oa_export_docx` | `src/routes/ai.rs:2773` |
| 113 | POST | `/api/oa/export` | — | 任意 JSON | — | HTTP 响应（动态，状态码/正文见源码） | `routes::oa_export::api_oa_export_post` | `src/routes/oa_export.rs:313` |
| 114 | GET | `/api/oa/export/{filename}` | `filename` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::oa_export::api_oa_export_get` | `src/routes/oa_export.rs:324` |
| 115 | GET | `/api/oa/rounds/{patent_id}` | `patent_id` | — | — | JSON（§115） | `routes::api_oa_rounds_get` | `src/routes/oa_rounds.rs:15` |
| 116 | POST | `/api/oa/rounds` | — | 任意 JSON（§116） | — | JSON（§116） | `routes::api_oa_rounds_create` | `src/routes/oa_rounds.rs:41` |
| 117 | PUT | `/api/oa/rounds/update/{id}` | `id` | 任意 JSON（§117） | — | JSON（§117） | `routes::api_oa_rounds_update` | `src/routes/oa_rounds.rs:156` |
| 118 | GET | `/api/oa/timeline/{patent_id}` | `patent_id` | — | — | JSON（§118） | `routes::api_oa_timeline` | `src/routes/oa_rounds.rs:231` |
| 119 | POST | `/api/ai/check-amendments` | — | 任意 JSON（§119） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_check_amendments` | `src/routes/ai.rs:2667` |
| 120 | POST | `/api/ai/threat-assessment` | — | 任意 JSON（§120） | — | JSON（§120） | `routes::api_ai_threat_assessment` | `src/routes/ai.rs:2704` |
| 121 | POST | `/api/ai/claim-chart` | — | 任意 JSON（§121） | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_ai_claim_chart` | `src/routes/ai.rs:2735` |
| 122 | GET | `/api/ai/cost` | — | — | — | JSON（§122） | `routes::api_ai_cost_summary` | `src/routes/ai.rs:2810` |
| 123 | GET | `/api/ai/cost/records` | — | — | — | JSON（§123） | `routes::api_ai_cost_records` | `src/routes/ai.rs:2821` |
| 124 | POST | `/api/ai/cost/record` | — | 任意 JSON（§124） | — | JSON（§124） | `routes::api_ai_cost_save` | `src/routes/ai.rs:2836` |
| 125 | POST | `/api/idea/submit` | — | `IdeaSubmitRequest` | — | JSON（§125） | `routes::api_idea_submit` | `src/routes/idea.rs:171` |
| 126 | POST | `/api/idea/analyze` | — | 任意 JSON（§126） | — | JSON（§126） | `routes::api_idea_analyze` | `src/routes/idea.rs:219` |
| 127 | POST | `/api/idea/pipeline` | — | 任意 JSON（§127） | — | JSON（§127） | `routes::api_idea_pipeline` | `src/routes/idea.rs:997` |
| 128 | POST | `/api/ideas/batch-compare` | — | 任意 JSON（§128） | — | JSON（§128） | `routes::api_ideas_batch_compare` | `src/routes/idea.rs:1855` |
| 129 | GET | `/api/idea/list` | — | — | — | JSON（§129） | `routes::api_idea_list` | `src/routes/idea.rs:399` |
| 130 | GET | `/api/idea/{id}` | `id` | — | — | JSON（§130） | `routes::api_idea_get` | `src/routes/idea.rs:367` |
| 131 | POST | `/api/idea/{id}/delete` | `id` | — | — | JSON（§131） | `routes::api_idea_delete` | `src/routes/idea.rs:409` |
| 132 | GET | `/api/idea/{id}/progress` | `id` | — | — | SSE 事件流（text/event-stream） | `routes::api_idea_progress` | `src/routes/idea.rs:1216` |
| 133 | POST | `/api/idea/{id}/resume` | `id` | — | — | JSON（§133） | `routes::api_idea_resume` | `src/routes/idea.rs:1156` |
| 134 | GET / POST | `/api/idea/{id}/research-state` | `id` | `ResearchStateUpdateRequest` | — | JSON（§134） | `routes::api_idea_research_state`<br>`routes::api_idea_research_state_update` | `src/routes/idea.rs:2068`<br>`src/routes/idea.rs:2087` |
| 135 | POST | `/api/idea/{id}/redirect` | `id` | `RedirectRequest` | — | JSON（§135） | `routes::api_idea_redirect` | `src/routes/idea.rs:2108` |
| 136 | GET | `/api/idea/{id}/report` | `id` | — | 任意键值对（§136） | JSON（§136） | `routes::api_idea_report` | `src/routes/idea.rs:1267` |
| 137 | GET | `/api/idea/{id}/report.html` | `id` | — | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_idea_report_html` | `src/routes/idea.rs:1419` |
| 138 | GET | `/api/ai/models` | — | — | 任意键值对（§138） | JSON（§138） | `routes::list_ai_models` | `src/routes/idea.rs:2474` |
| 139 | POST | `/api/idea/{id}/chat` | `id` | `IdeaChatRequest` | — | JSON（§139） | `routes::api_idea_chat` | `src/routes/idea.rs:546` |
| 140 | GET | `/api/idea/{id}/evidence` | `id` | — | — | JSON（§140） | `routes::api_idea_evidence` | `src/routes/idea.rs:438` |
| 141 | GET | `/api/idea/{id}/kernel-report` | `id` | — | — | JSON（§141） | `routes::api_idea_kernel_report` | `src/routes/idea.rs:455` |
| 142 | GET | `/api/idea/{id}/messages` | `id` | — | — | JSON（§142） | `routes::api_idea_messages` | `src/routes/idea.rs:971` |
| 143 | GET | `/api/idea/{id}/chat/conclusions` | `id` | — | — | JSON（§143） | `routes::api_idea_chat_conclusions` | `src/routes/idea.rs:1730` |
| 144 | GET | `/api/idea/{id}/chat/conversation` | `id` | — | — | JSON（§144） | `routes::api_idea_chat_conversation` | `src/routes/idea.rs:1709` |
| 145 | POST | `/api/idea/{id}/summarize` | `id` | — | — | JSON（§145） | `routes::api_idea_summarize_discussion` | `src/routes/idea.rs:1798` |
| 146 | GET / POST | `/api/ideas/{id}/feature-cards` | `id` | `CreateFeatureCardRequest` | — | HTTP 响应（动态，状态码/正文见源码） | `routes::api_get_feature_cards`<br>`routes::api_create_feature_card` | `src/routes/feature_cards.rs:16`<br>`src/routes/feature_cards.rs:50` |
| 147 | GET | `/api/feature-cards/diff` | — | — | 任意键值对（§147） | HTTP 响应（动态，状态码/正文见源码） | `routes::api_feature_card_diff` | `src/routes/feature_cards.rs:121` |
| 148 | GET | `/api/idea/{id}/claim-tree` | `id` | — | — | JSON（§148） | `routes::api_idea_claim_tree` | `src/routes/idea.rs:2221` |
| 149 | POST | `/api/idea/{id}/iterate` | `id` | — | — | JSON（§149） | `routes::api_idea_iterate` | `src/routes/idea.rs:1955` |
| 150 | GET | `/api/idea/{id}/versions` | `id` | — | — | JSON（§150） | `routes::api_idea_versions` | `src/routes/idea.rs:2290` |
| 151 | GET | `/api/idea/{id}/branches` | `id` | — | — | JSON（§151） | `routes::api_idea_branches` | `src/routes/idea.rs:2315` |
| 152 | GET | `/api/idea/{id}/findings` | `id` | — | — | JSON（§152） | `routes::api_idea_findings` | `src/routes/idea.rs:2326` |
| 153 | GET / POST | `/api/idea/{id}/memory` | `id` | POST 任意 JSON（§153） | — | JSON（§153） | `routes::api_idea_memory`<br>`routes::api_idea_memory_add` | `src/routes/idea.rs:2337`<br>`src/routes/idea.rs:2358` |
| 154 | DELETE | `/api/idea/{id}/memory/{entry_id}` | `id`, `entry_id` | — | — | JSON（§154） | `routes::api_idea_memory_delete` | `src/routes/idea.rs:2416` |
| 155 | GET | `/api/ipc/tree` | — | — | — | JSON（§155） | `routes::api_ipc_tree` | `src/routes/ipc.rs:22` |
| 156 | GET | `/api/ipc/{code}/patents` | `code` | — | — | JSON（§156） | `routes::api_ipc_patents` | `src/routes/ipc.rs:122` |
| 157 | POST | `/api/patents/import` | — | `ImportRequest` | — | JSON（§157） | `routes::api_import_patents` | `src/routes/settings.rs:802` |
| 158 | GET / POST | `/api/collections` | — | POST 任意 JSON（§158） | — | JSON（§158） | `routes::api_list_collections`<br>`routes::api_create_collection` | `src/routes/collections.rs:28`<br>`src/routes/collections.rs:8` |
| 159 | DELETE | `/api/collections/{id}` | `id` | — | — | JSON（§159） | `routes::api_delete_collection` | `src/routes/collections.rs:49` |
| 160 | GET | `/api/collections/{id}/patents` | `id` | — | — | JSON（§160） | `routes::api_get_collection_patents` | `src/routes/collections.rs:92` |
| 161 | POST | `/api/collections/{id}/add` | `id` | 任意 JSON（§161） | — | JSON（§161） | `routes::api_add_to_collection` | `src/routes/collections.rs:59` |
| 162 | DELETE | `/api/collections/{id}/remove/{patent_id}` | `id`, `patent_id` | — | — | JSON（§162） | `routes::api_remove_from_collection` | `src/routes/collections.rs:82` |
| 163 | GET / POST | `/api/patents/{id}/tags` | `id` | POST 任意 JSON（§163） | — | JSON（§163） | `routes::api_get_patent_tags`<br>`routes::api_add_tag` | `src/routes/collections.rs:128`<br>`src/routes/collections.rs:102` |
| 164 | DELETE | `/api/patents/{id}/tags/{tag}` | `id`, `tag` | — | — | JSON（§164） | `routes::api_remove_tag` | `src/routes/collections.rs:118` |
| 165 | GET | `/api/patents/{id}/collections` | `id` | — | — | JSON（§165） | `routes::api_get_patent_collections` | `src/routes/collections.rs:151` |
| 166 | GET | `/api/tags` | — | — | — | JSON（§166） | `routes::api_list_all_tags` | `src/routes/collections.rs:138` |
| 167 | POST | `/api/upload/compare` | — | — | — | JSON（§167） | `routes::api_upload_compare` | `src/routes/upload.rs:192` |
| 168 | POST | `/api/upload/extract` | — | — | — | JSON（§168） | `routes::api_upload_extract` | `src/routes/upload.rs:370` |
| 169 | POST | `/api/upload/pdf-store` | — | — | — | JSON（§169） | `routes::api_upload_pdf_store` | `src/routes/upload.rs:127` |

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

### `AuthUnlockRequest`（请求体；定义于 `src/app.rs:85`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `access_code` | `String` | 否 |

使用该类型的端点：`/api/auth/unlock`

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

使用该类型的端点：`/api/chat/{session_key}`

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

使用该类型的端点：`/api/ideas/{id}/feature-cards`

### `DissectAnalyzeReq`（请求体；定义于 `src/cards/deep_dissect/routes.rs:67`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `doc_id` | `String` | 否 |
| `mode` | `Option<String>` | 是 |
| `sub_types` | `Vec<String>` | 是 |
| `privacy_offline_only` | `bool` | 是 |

使用该类型的端点：`/api/deep-dissect/analyze`

### `DissectAskReq`（请求体；定义于 `src/cards/deep_dissect/routes.rs:89`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `doc_id` | `String` | 否 |
| `question` | `String` | 否 |
| `selected_excerpt` | `Option<String>` | 是 |
| `selected_anchor` | `Option<String>` | 是 |
| `figure_numeral` | `Option<String>` | 是 |
| `image_data_urls` | `Vec<String>` | 是 |
| `privacy_offline_only` | `bool` | 是 |

使用该类型的端点：`/api/deep-dissect/ask`

### `DissectIngestReq`（请求体；定义于 `src/cards/deep_dissect/routes.rs:22`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `doc_id` | `Option<String>` | 是 |
| `doc_kind` | `Option<String>` | 是 |
| `asset_role` | `Option<String>` | 是 |
| `filing_status` | `Option<String>` | 是 |
| `external_id` | `Option<String>` | 是 |
| `title` | `Option<String>` | 是 |
| `authors_or_assignees` | `Option<String>` | 是 |
| `pub_or_filing_date` | `Option<String>` | 是 |
| `venue_or_authority` | `Option<String>` | 是 |
| `abstract_text` | `Option<String>` | 是 |
| `claims_json` | `Option<String>` | 是 |
| `fulltext_raw` | `Option<String>` | 是 |
| `filename` | `Option<String>` | 是 |
| `mime_type` | `Option<String>` | 是 |
| `file_base64` | `Option<String>` | 是 |
| `privacy_offline_only` | `bool` | 是 |

使用该类型的端点：`/api/deep-dissect/ingest`

### `FetchPatentRequest`（请求体；定义于 `src/types/patent.rs:107`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `patent_number` | `String` | 否 |
| `source` | `Option<String>` | 是 |

使用该类型的端点：`/api/patent/fetch`、`/api/ai/summarize`

### `ForgeCompileReq`（请求体；定义于 `src/cards/deliverable_forge/routes.rs:36`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `String` | 否 |
| `genre` | `DeliverableGenre` | 否 |
| `related_doc_ids` | `Vec<String>` | 是 |
| `reference_doc_ids` | `Vec<String>` | 是 |
| `experiment_ids` | `Vec<String>` | 是 |
| `knowhow_markers` | `Vec<String>` | 是 |
| `custom_notes` | `String` | 是 |
| `use_ai` | `bool` | 是 |

使用该类型的端点：`/api/deliverable-forge/compile`

### `ForgeExportReq`（请求体；定义于 `src/cards/deliverable_forge/routes.rs:55`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `String` | 否 |
| `genre` | `DeliverableGenre` | 否 |
| `format` | `DeliverableFormat` | 否 |
| `sections` | `Vec<(String, String)>` | 是 |
| `original_text_for_diff` | `Option<String>` | 是 |
| `amended_text_for_diff` | `Option<String>` | 是 |
| `related_doc_ids` | `Vec<String>` | 是 |
| `reference_doc_ids` | `Vec<String>` | 是 |
| `knowhow_markers` | `Vec<String>` | 是 |
| `acknowledge_priority_warning` | `bool` | 是 |
| `bundle_zip` | `bool` | 是 |

使用该类型的端点：`/api/deliverable-forge/export`

### `ForgePriorityCheckReq`（请求体；定义于 `src/cards/deliverable_forge/routes.rs:24`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `related_doc_ids` | `Vec<String>` | 是 |
| `target_genre` | `DeliverableGenre` | 否 |
| `knowhow_markers` | `Vec<String>` | 是 |
| `draft_text` | `String` | 是 |

使用该类型的端点：`/api/deliverable-forge/priority-check`

### `IdeaChatRequest`（请求体；定义于 `src/types/idea.rs:40`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `message` | `String` | 否 |
| `depth` | `String` | 是 |
| `images` | `Vec<String>` | 是 |
| `attachments` | `Vec<TextAttachment>` | 是 |

使用该类型的端点：`/api/idea/{id}/chat`

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

### `OmniSearchChunksReq`（请求体；定义于 `src/cards/omni_search/routes.rs:48`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `query` | `String` | 否 |
| `mode` | `Option<SearchSceneMode>` | 是 |
| `doc_ids` | `Option<Vec<String>>` | 是 |
| `asset_role` | `Option<AssetRole>` | 是 |
| `top_k` | `Option<usize>` | 是 |
| `char_budget` | `Option<usize>` | 是 |

使用该类型的端点：`/api/omni-search/chunks`

### `OmniSearchExportReq`（请求体；定义于 `src/cards/omni_search/routes.rs:64`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `query` | `String` | 是 |
| `format` | `String` | 是 |
| `realms` | `Vec<SearchRealm>` | 是 |
| `asset_role` | `Option<AssetRole>` | 是 |
| `hits` | `Vec<OmniSearchHit>` | 是 |

使用该类型的端点：`/api/omni-search/export`

### `OmniSearchQueryReq`（请求体；定义于 `src/cards/omni_search/routes.rs:28`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `query` | `String` | 是 |
| `realms` | `Vec<SearchRealm>` | 是 |
| `asset_role` | `Option<AssetRole>` | 是 |
| `enable_ai_rewrite` | `bool` | 是 |
| `save_to_vault` | `bool` | 是 |
| `max_per_source` | `Option<usize>` | 是 |
| `save_doc` | `Option<KnowledgeDoc>` | 是 |

使用该类型的端点：`/api/omni-search/query`

### `ProbeProviderReq`（请求体；定义于 `src/cards/settings_vault/service.rs:72`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `provider_id` | `Option<String>` | 是 |
| `base_url` | `Option<String>` | 是 |
| `api_key` | `Option<String>` | 是 |

使用该类型的端点：`/api/settings/probe-provider`

### `RebuttalAddRoundReq`（请求体；定义于 `src/cards/rebuttal_hub/routes.rs:62`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `round_label` | `Option<String>` | 是 |
| `oa_type` | `Option<String>` | 是 |
| `dispatch_date` | `Option<String>` | 是 |
| `notice_text` | `String` | 否 |
| `strategy_used` | `Option<String>` | 是 |
| `draft_text` | `Option<String>` | 是 |
| `notes` | `Option<String>` | 是 |

使用该类型的端点：`/api/rebuttal-hub/case/{id}/round`

### `RebuttalCreateCaseReq`（请求体；定义于 `src/cards/rebuttal_hub/routes.rs:45`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `mode` | `String` | 否 |
| `title` | `String` | 否 |
| `own_doc_id` | `Option<String>` | 是 |
| `reference_doc_ids` | `Vec<String>` | 否 |
| `experiment_ids` | `Vec<String>` | 否 |
| `oa_type` | `Option<String>` | 是 |
| `dispatch_date` | `Option<String>` | 是 |
| `original_claim_or_text` | `String` | 否 |
| `amended_claim_or_text` | `String` | 否 |
| `notice_or_review_text` | `String` | 否 |
| `knowhow_markers` | `Vec<String>` | 否 |

使用该类型的端点：`/api/rebuttal-hub/case`

### `RebuttalDraftReq`（请求体；定义于 `src/cards/rebuttal_hub/routes.rs:82`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `preferred_track` | `String` | 否 |
| `user_instruction` | `String` | 否 |

使用该类型的端点：`/api/rebuttal-hub/case/{id}/draft`

### `RebuttalExportReq`（请求体；定义于 `src/cards/rebuttal_hub/routes.rs:100`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `format` | `Option<String>` | 是 |
| `acknowledge_priority_warning` | `bool` | 否 |

使用该类型的端点：`/api/rebuttal-hub/case/{id}/export`

### `RebuttalMockReviewReq`（请求体；定义于 `src/cards/rebuttal_hub/routes.rs:75`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `focus_query` | `String` | 否 |

使用该类型的端点：`/api/rebuttal-hub/case/{id}/mock-review`

### `RebuttalVerifyReq`（请求体；定义于 `src/cards/rebuttal_hub/routes.rs:90`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `draft_text` | `Option<String>` | 是 |
| `original_claim_or_text` | `Option<String>` | 是 |
| `amended_claim_or_text` | `Option<String>` | 是 |
| `require_citations` | `bool` | 否 |

使用该类型的端点：`/api/rebuttal-hub/case/{id}/verify`

### `RedirectRequest`（请求体；定义于 `src/routes/idea.rs:39`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `restart_from` | `Option<String>` | 是 |
| `technical_domain` | `Option<String>` | 是 |
| `add_queries` | `Option<Vec<String>>` | 是 |
| `reason` | `Option<String>` | 是 |
| `research_state` | `Option<ResearchStateUpdateRequest>` | 是 |

使用该类型的端点：`/api/idea/{id}/redirect`

### `ResearchStateUpdateRequest`（请求体；定义于 `src/routes/idea.rs:31`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `current_hypothesis` | `Option<String>` | 是 |
| `excluded_paths` | `Option<Vec<String>>` | 是 |
| `open_questions` | `Option<Vec<String>>` | 是 |
| `verified_claims` | `Option<Vec<String>>` | 是 |

使用该类型的端点：`/api/idea/{id}/research-state`

### `ResolveTitlesRequest`（请求体；定义于 `src/routes/patent.rs:352`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `patents` | `Vec<ResolveTitleItem>` | 否 |

使用该类型的端点：`/api/patent/resolve-chinese-titles`

### `SchemeMatrixCompareReq`（请求体；定义于 `src/cards/scheme_matrix/routes.rs:23`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `Option<String>` | 是 |
| `mode` | `Option<String>` | 是 |
| `sources` | `Vec<SchemeSourceInput>` | 否 |
| `include_ai_synthesis` | `Option<bool>` | 是 |

使用该类型的端点：`/api/scheme-matrix/compare`

### `SchemeMatrixExportReq`（请求体；定义于 `src/cards/scheme_matrix/routes.rs:35`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `format` | `Option<String>` | 是 |
| `mode` | `Option<String>` | 是 |
| `acknowledge_priority_warning` | `Option<bool>` | 是 |

使用该类型的端点：`/api/scheme-matrix/{id}/export`

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

### `SettingsConfigUpdateReq`（请求体；定义于 `src/cards/settings_vault/service.rs:38`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `active_provider` | `Option<String>` | 是 |
| `ai_base_url` | `Option<String>` | 是 |
| `ai_model` | `Option<String>` | 是 |
| `ai_model_expert` | `Option<String>` | 是 |
| `ai_model_fast` | `Option<String>` | 是 |
| `ai_api_key` | `Option<String>` | 是 |
| `provider_keys` | `Option<Vec<ProviderKeyUpdate>>` | 是 |
| `failover_order` | `Option<Vec<String>>` | 是 |
| `vision_provider_id` | `Option<String>` | 是 |
| `daily_budget` | `Option<usize>` | 是 |
| `privacy_offline_only` | `Option<bool>` | 是 |
| `umi_ocr_url` | `Option<String>` | 是 |
| `serpapi_keys` | `Option<Vec<String>>` | 是 |
| `epo_key` | `Option<String>` | 是 |
| `epo_secret` | `Option<String>` | 是 |

使用该类型的端点：`/api/settings/config`

### `StudioCheckpointReq`（请求体；定义于 `src/cards/idea_studio/routes.rs:80`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `step_key` | `String` | 是 |
| `confirmed_output` | `serde_json::Value` | 是 |

使用该类型的端点：`/api/idea-studio/run/{id}/checkpoint`

### `StudioCreateSessionReq`（请求体；定义于 `src/cards/idea_studio/routes.rs:31`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `Option<String>` | 是 |
| `mounted_own_doc_ids` | `Vec<String>` | 是 |

使用该类型的端点：`/api/idea-studio/session`

### `StudioExperimentReq`（请求体；定义于 `src/cards/idea_studio/routes.rs:89`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `exp_id` | `Option<String>` | 是 |
| `title` | `String` | 否 |
| `hypothesis` | `String` | 是 |
| `control_group_spec` | `String` | 是 |
| `test_group_spec` | `String` | 是 |
| `sandbox_output_json` | `Option<serde_json::Value>` | 是 |
| `measured_data_json` | `serde_json::Value` | 否 |
| `verified` | `bool` | 是 |

使用该类型的端点：`/api/idea-studio/run/{id}/experiment`

### `StudioRunForkReq`（请求体；定义于 `src/cards/idea_studio/routes.rs:70`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `fork_from_step` | `String` | 否 |
| `override_baseline` | `Option<SchemeBaselineCard>` | 是 |
| `route_label` | `Option<String>` | 是 |

使用该类型的端点：`/api/idea-studio/run/{id}/fork`

### `StudioRunStartReq`（请求体；定义于 `src/cards/idea_studio/routes.rs:57`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `session_id` | `Option<String>` | 是 |
| `tier_mode` | `String` | 是 |
| `scheme_baseline` | `Option<SchemeBaselineCard>` | 是 |
| `mounted_own_doc_ids` | `Vec<String>` | 是 |

使用该类型的端点：`/api/idea-studio/run/start`

### `StudioTurnReq`（请求体；定义于 `src/cards/idea_studio/routes.rs:40`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `user_text` | `String` | 是 |
| `attachments` | `Vec<IngestedAttachment>` | 是 |
| `mounted_own_doc_ids` | `Vec<String>` | 是 |
| `lock_baseline` | `bool` | 是 |

使用该类型的端点：`/api/idea-studio/session/{id}/turn`

### `VaultImportPackReq`（请求体；定义于 `src/cards/settings_vault/service.rs:82`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `format` | `Option<String>` | 是 |
| `filename` | `Option<String>` | 是 |
| `content` | `String` | 是 |
| `default_doc_kind` | `Option<String>` | 是 |
| `default_asset_role` | `Option<String>` | 是 |
| `default_filing_status` | `Option<String>` | 是 |

使用该类型的端点：`/api/vault/import-pack`

### `VaultPatchDocReq`（请求体；定义于 `src/cards/settings_vault/service.rs:112`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `doc_id` | `String` | 否 |
| `asset_role` | `String` | 否 |
| `filing_status` | `String` | 否 |

使用该类型的端点：`/api/vault/docs`

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

### `IngestedAttachment`（struct；被 `StudioTurnReq` 引用；定义于 `src/core/multimodal/doc_ingest.rs:41`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `attachment_id` | `String` | 否 |
| `filename` | `String` | 否 |
| `mime_type` | `String` | 否 |
| `extracted_text` | `String` | 否 |
| `tier_used` | `IngestTierUsed` | 否 |
| `page_count` | `usize` | 否 |
| `figure_numerals` | `Vec<FigureNumeral>` | 否 |
| `structured_summary_json` | `Option<serde_json::Value>` | 是 |
| `warnings` | `Vec<String>` | 是 |

### `KnowledgeDoc`（struct；被 `OmniSearchQueryReq` 引用；定义于 `src/core/vault/models.rs:115`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `doc_id` | `String` | 否 |
| `doc_kind` | `DocKind` | 否 |
| `asset_role` | `AssetRole` | 是 |
| `external_id` | `String` | 是 |
| `title_zh` | `String` | 是 |
| `title_en` | `String` | 是 |
| `authors_or_assignees` | `String` | 是 |
| `pub_or_filing_date` | `String` | 是 |
| `venue_or_authority` | `String` | 是 |
| `filing_status` | `FilingStatus` | 是 |
| `doi_verified` | `bool` | 是 |
| `bibtex_raw` | `String` | 是 |
| `abstract_text` | `String` | 是 |
| `claims_json` | `String` | 是 |
| `fulltext_raw` | `String` | 是 |
| `fulltext_status` | `String` | 是 |
| `chunk_ver` | `i32` | 是 |

### `OmniSearchHit`（struct；被 `OmniSearchExportReq` 引用；定义于 `src/core/vault/models.rs:432`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `doc` | `KnowledgeDoc` | 否 |
| `realm` | `SearchRealm` | 否 |
| `source_provider` | `String` | 否 |
| `relevance_score` | `f64` | 否 |
| `match_reason` | `String` | 否 |
| `source_url` | `String` | 否 |

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

### `ProviderKeyUpdate`（struct；被 `SettingsConfigUpdateReq` 引用；定义于 `src/cards/settings_vault/service.rs:31`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `String` | 否 |
| `api_key` | `Option<String>` | 是 |

### `ResolveTitleItem`（struct；被 `ResolveTitlesRequest` 引用；定义于 `src/routes/patent.rs:357`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `id` | `String` | 否 |
| `patent_number` | `Option<String>` | 是 |

### `SchemeBaselineCard`（struct；被 `StudioRunStartReq`、`StudioRunForkReq` 引用；定义于 `src/core/multimodal/session.rs:22`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `title` | `String` | 否 |
| `core_problem` | `String` | 否 |
| `technical_means` | `Vec<String>` | 否 |
| `key_components` | `Vec<FigureNumeral>` | 否 |
| `expected_effects` | `Vec<String>` | 否 |
| `experiment_metrics` | `Vec<String>` | 否 |
| `mounted_own_doc_ids` | `Vec<String>` | 否 |
| `mermaid_diagram` | `Option<String>` | 是 |
| `unresolved_questions` | `Vec<String>` | 否 |
| `locked` | `bool` | 否 |

### `SchemeSourceInput`（struct；被 `SchemeMatrixCompareReq` 引用；定义于 `src/cards/scheme_matrix/service.rs:62`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `label` | `String` | 是 |
| `doc_id` | `Option<String>` | 是 |
| `run_id` | `Option<String>` | 是 |
| `inline_text` | `Option<String>` | 是 |
| `metrics` | `Option<serde_json::Value>` | 是 |
| `role` | `Option<String>` | 是 |

### `TextAttachment`（struct；被 `IdeaChatRequest` 引用；定义于 `src/types/idea.rs:34`）

| 字段（JSON 名） | 类型 | 可省略 |
| --- | --- | --- |
| `name` | `String` | 否 |
| `content` | `String` | 否 |

### `AssetRole`（enum；被 `OmniSearchQueryReq`、`OmniSearchChunksReq`、`OmniSearchExportReq` 引用；定义于 `src/core/vault/models.rs:57`）

取值：`own_asset`（Rust: `OwnAsset`）、`competitor_wall`（Rust: `CompetitorWall`）、`review_cited`（Rust: `ReviewCited`）、`general_ref`（Rust: `GeneralRef`）

### `CadContextKind`（enum；被 `CadDrawRequest`、`CadHistoryQuery` 引用；定义于 `src/patent.rs:34`）

取值：`idea`（Rust: `Idea`）、`patent`（Rust: `Patent`）、`oa`（Rust: `Oa`）

### `DeliverableFormat`（enum；被 `ForgeExportReq` 引用；定义于 `src/core/vault/models.rs:521`）

取值：`docx`（Rust: `Docx`）、`md`（Rust: `Md`）、`tex`（Rust: `Tex`）、`xlsx`（Rust: `Xlsx`）、`pdf`（Rust: `Pdf`）

### `DeliverableGenre`（enum；被 `ForgePriorityCheckReq`、`ForgeCompileReq`、`ForgeExportReq` 引用；定义于 `src/core/vault/models.rs:509`）

取值：`product_prd_trd`（Rust: `ProductPrdTrd`）、`survey_fto_report`（Rust: `SurveyFtoReport`）、`academic_paper`（Rust: `AcademicPaper`）、`patent_disclosure`（Rust: `PatentDisclosure`）、`oa_package`（Rust: `OaPackage`）、`paper_rebuttal_package`（Rust: `PaperRebuttalPackage`）

### `Lang`（enum；被 `SearchRequest` 引用；定义于 `src/search/model.rs:16`）

取值：`chinese`（Rust: `Chinese`）、`english`（Rust: `English`）、`all`（Rust: `All`）

### `SearchRealm`（enum；被 `OmniSearchQueryReq`、`OmniSearchExportReq` 引用；定义于 `src/core/vault/models.rs:423`）

取值：`patent`（Rust: `Patent`）、`scholar`（Rust: `Scholar`）、`code_repo`（Rust: `CodeRepo`）、`standard`（Rust: `Standard`）

### `SearchSceneMode`（enum；被 `OmniSearchChunksReq` 引用；定义于 `src/core/vault/models.rs:395`）

取值：`patent_compare`（Rust: `PatentCompare`）、`paper_survey`（Rust: `PaperSurvey`）、`anchor_exact`（Rust: `AnchorExact`）

## 字段抽取明细

每条路由只列**能从源码里直接看到**的东西：函数体内读取的键（`req["x"]` / `req.get("x")` / `get_*(&req, "x")`）
与 `json!({...})` 返回的**顶层键**。嵌套结构、经共享 helper 读取的键、运行时拼接的键不在此列——抽不到就写「未抽取到」。

### §1 GET `/api/healthz`

- 源码：`src/app.rs:174`
- 响应顶层键：`service`、`status`、`version`
- `status` 取值：ok

### §2 POST `/api/auth/unlock`

- 源码：`src/app.rs:183`
- 请求体：`AuthUnlockRequest`（字段见上文类型表）
- 响应顶层键：`authenticated`、`code`、`message`、`status`
- `status` 取值：error / ok

### §4 GET / POST `/api/settings/config`

- **GET** `crate::cards::settings_vault::api_settings_get_config`（`src/cards/settings_vault/routes.rs:30`）
  - 响应顶层键：未抽取到（动态构造或非 JSON 响应）
- **POST** `crate::cards::settings_vault::api_settings_save_config`（`src/cards/settings_vault/routes.rs:35`）
  - 请求体：`SettingsConfigUpdateReq`（字段见上文类型表）
  - 响应顶层键：`code`、`message`、`status`
  - `status` 取值：error

### §6 POST `/api/vault/import-pack`

- 源码：`src/cards/settings_vault/routes.rs:60`
- 请求体：`VaultImportPackReq`（字段见上文类型表）
- 响应顶层键：`code`、`message`、`status`
- `status` 取值：error

### §7 GET / DELETE / PATCH `/api/vault/docs`

- **GET** `crate::cards::settings_vault::api_vault_list_docs`（`src/cards/settings_vault/routes.rs:77`）
  - 响应顶层键：未抽取到（动态构造或非 JSON 响应）
- **DELETE** `crate::cards::settings_vault::api_vault_delete_doc`（`src/cards/settings_vault/routes.rs:102`）
  - 响应顶层键：`code`、`message`、`status`
  - `status` 取值：error
- **PATCH** `crate::cards::settings_vault::api_vault_patch_doc`（`src/cards/settings_vault/routes.rs:85`）
  - 请求体：`VaultPatchDocReq`（字段见上文类型表）
  - 响应顶层键：`code`、`message`、`status`
  - `status` 取值：error

### §8 POST `/api/omni-search/query`

- 源码：`src/cards/omni_search/routes.rs:97`
- 请求体：`OmniSearchQueryReq`（字段见上文类型表）
- 响应顶层键：`diagnostics`、`hits`、`plan`、`saved_count`、`saved_doc`、`status`、`total_before_dedup`
- `status` 取值：ok

### §9 POST `/api/omni-search/chunks`

- 源码：`src/cards/omni_search/routes.rs:155`
- 请求体：`OmniSearchChunksReq`（字段见上文类型表）
- 响应顶层键：`bundle`、`status`
- `status` 取值：ok

### §10 GET `/api/omni-search/doc/{id}`

- 源码：`src/cards/omni_search/routes.rs:195`
- 响应顶层键：`doc`、`doc_chunks`、`figure_numerals`、`status`
- `status` 取值：ok

### §11 POST `/api/omni-search/export`

- 源码：`src/cards/omni_search/routes.rs:224`
- 请求体：`OmniSearchExportReq`（字段见上文类型表）
- 响应顶层键：`code`、`message`、`status`
- `status` 取值：error

### §12 POST `/api/idea-studio/session`

- 源码：`src/cards/idea_studio/routes.rs:135`
- 请求体：`StudioCreateSessionReq`（字段见上文类型表）
- 响应顶层键：`code`、`message`、`session`、`session_id`、`status`
- `status` 取值：error / ok

### §13 POST `/api/idea-studio/session/{id}/turn`

- 源码：`src/cards/idea_studio/routes.rs:154`
- 请求体：`StudioTurnReq`（字段见上文类型表）
- 响应顶层键：`baseline`、`follow_up_questions`、`isolated_prompt`、`reply_message`、`session`、`session_id`、`status`
- `status` 取值：ok

### §14 POST `/api/idea-studio/run/start`

- 源码：`src/cards/idea_studio/routes.rs:181`
- 请求体：`StudioRunStartReq`（字段见上文类型表）
- 响应顶层键：`nodes`、`run`、`run_id`、`status`
- `status` 取值：ok

### §15 POST `/api/idea-studio/run/{id}/fork`

- 源码：`src/cards/idea_studio/routes.rs:204`
- 请求体：`StudioRunForkReq`（字段见上文类型表）
- 响应顶层键：`fork_from_step`、`genealogy`、`nodes`、`parent_run_id`、`run`、`run_id`、`status`
- `status` 取值：ok

### §16 POST `/api/idea-studio/run/{id}/checkpoint`

- 源码：`src/cards/idea_studio/routes.rs:234`
- 请求体：`StudioCheckpointReq`（字段见上文类型表）
- 响应顶层键：`nodes`、`run`、`run_id`、`status`
- `status` 取值：ok

### §17 POST `/api/idea-studio/run/{id}/experiment`

- 源码：`src/cards/idea_studio/routes.rs:258`
- 请求体：`StudioExperimentReq`（字段见上文类型表）
- 响应顶层键：`evidence_anchor`、`exp_id`、`experiment`、`run_id`、`status`
- `status` 取值：ok

### §19 POST `/api/deep-dissect/ingest`

- 源码：`src/cards/deep_dissect/routes.rs:135`
- 请求体：`DissectIngestReq`（字段见上文类型表）
- 响应顶层键：`chunks`、`doc`、`existing_feature_atoms`、`figure_numerals`、`ingest_tier_used`、`status`、`warnings`
- `status` 取值：ok

### §20 POST `/api/deep-dissect/analyze`

- 源码：`src/cards/deep_dissect/routes.rs:174`
- 请求体：`DissectAnalyzeReq`（字段见上文类型表）
- 响应顶层键：`detected_mode`、`doc_id`、`figure_numerals`、`report_markdown`、`saved_feature_atoms`、`sections`、`status`、`verified_citations`
- `status` 取值：ok

### §21 POST `/api/deep-dissect/ask`

- 源码：`src/cards/deep_dissect/routes.rs:214`
- 请求体：`DissectAskReq`（字段见上文类型表）
- 响应顶层键：`answer_markdown`、`answer_source`、`doc_id`、`matched_chunks`、`matched_numerals`、`provider_id`、`status`、`verified_citations`
- `status` 取值：ok

### §22 POST `/api/scheme-matrix/compare`

- 源码：`src/cards/scheme_matrix/routes.rs:45`
- 请求体：`SchemeMatrixCompareReq`（字段见上文类型表）
- 响应顶层键：`code`、`matrix`、`matrix_id`、`message`、`status`
- `status` 取值：error / ok

### §23 GET `/api/scheme-matrix/{id}`

- 源码：`src/cards/scheme_matrix/routes.rs:80`
- 响应顶层键：`code`、`matrix`、`matrix_id`、`message`、`status`
- `status` 取值：error / ok

### §24 POST `/api/scheme-matrix/{id}/export`

- 源码：`src/cards/scheme_matrix/routes.rs:113`
- 请求体：`SchemeMatrixExportReq`（字段见上文类型表）
- 响应顶层键：`code`、`message`、`status`
- `status` 取值：error

### §25 POST `/api/rebuttal-hub/case`

- 源码：`src/cards/rebuttal_hub/routes.rs:106`
- 请求体：`RebuttalCreateCaseReq`（字段见上文类型表）
- 响应顶层键：`case`、`status`
- `status` 取值：ok

### §26 POST `/api/rebuttal-hub/case/{id}/round`

- 源码：`src/cards/rebuttal_hub/routes.rs:143`
- 请求体：`RebuttalAddRoundReq`（字段见上文类型表）
- 响应顶层键：`case`、`round`、`status`
- `status` 取值：ok

### §27 POST `/api/rebuttal-hub/case/{id}/mock-review`

- 源码：`src/cards/rebuttal_hub/routes.rs:169`
- 请求体：`RebuttalMockReviewReq`（字段见上文类型表）
- 响应顶层键：`case_id`、`deadline_info`、`issues`、`mode`、`offline_fallback`、`provider_used`、`status`
- `status` 取值：ok

### §28 POST `/api/rebuttal-hub/case/{id}/draft`

- 源码：`src/cards/rebuttal_hub/routes.rs:191`
- 请求体：`RebuttalDraftReq`（字段见上文类型表）
- 响应顶层键：`case_id`、`draft`、`mode`、`status`
- `status` 取值：ok

### §29 POST `/api/rebuttal-hub/case/{id}/verify`

- 源码：`src/cards/rebuttal_hub/routes.rs:209`
- 请求体：`RebuttalVerifyReq`（字段见上文类型表）
- 响应顶层键：`case_id`、`gate_report`、`mode`、`priority_check`、`scope_report`、`status`、`stitching_alerts`
- `status` 取值：ok

### §30 POST `/api/rebuttal-hub/case/{id}/export`

- 源码：`src/cards/rebuttal_hub/routes.rs:236`
- 请求体：`RebuttalExportReq`（字段见上文类型表）
- 响应顶层键：`code`、`message`、`status`
- `status` 取值：error

### §31 POST `/api/deliverable-forge/priority-check`

- 源码：`src/cards/deliverable_forge/routes.rs:93`
- 请求体：`ForgePriorityCheckReq`（字段见上文类型表）
- 响应顶层键：`priority_check`、`redacted_preview`、`status`
- `status` 取值：ok

### §32 POST `/api/deliverable-forge/compile`

- 源码：`src/cards/deliverable_forge/routes.rs:121`
- 请求体：`ForgeCompileReq`（字段见上文类型表）
- 响应顶层键：`preview`、`status`
- `status` 取值：ok

### §34 POST `/api/multimodal/upload`

- 源码：`src/cards/multimodal_api.rs:20`
- 响应顶层键：`attachment_id`、`code`、`extracted_text`、`figure_numerals`、`filename`、`message`、`mime_type`、`status`、`structured_summary`、`task_id`、`tier_used`
- `status` 取值：error / ok

### §35 GET `/api/multimodal/task/{id}/events`

- 源码：`src/cards/multimodal_api.rs:95`
- 响应顶层键：`ready`、`task_id`

### §39 GET `/api/cad/artifacts`

- 源码：`src/routes/cad.rs:165`
- 响应顶层键：`artifacts`

### §42 GET `/api/version`

- 源码：`src/routes/settings.rs:57`
- 响应顶层键：`build_time`、`db_path`、`git_branch`、`git_commit`、`templates`、`version`

### §43 GET `/api/oa/deadline-table`

- 源码：`src/oa_deadline.rs:158`
- 响应顶层键：`deadline_table`、`kind`、`valid_types`、`value`

### §44 GET `/api/oa/compute-deadline`

- 源码：`src/oa_deadline.rs:176`
- 响应顶层键：`error`

### §45 GET `/api/settings`

- 源码：`src/routes/settings.rs:78`
- 响应顶层键：`ai_api_key`、`ai_api_key_anthropic`、`ai_api_key_configured`、`ai_api_key_deepseek`、`ai_api_key_gemini`、`ai_api_key_openai`、`ai_api_key_openrouter`、`ai_api_key_qwen`、`ai_api_key_sensetime`、`ai_api_key_xiaomi`、`ai_api_key_zhipu`、`ai_base_url`、`ai_model`、`ai_model_expert`、`aioncad_workspace`、`epo_configured`、`epo_key`、`epo_key_configured`、`epo_secret`、`epo_secret_configured`、`freecad_auto_start`、`freecad_platform_supported`、`gemini_cli_available`、`gemini_cli_enabled`、`gemini_cli_path`、`google_auth_mode`、`google_client_id`、`google_client_id_set`、`google_client_secret_set`、`google_oauth_connected`、`is_ollama`、`search_llm_daily_budget`、`search_rerank_enabled`、`serpapi_key_configured`、`serpapi_keys`、`umi_ocr_base_url`

### §46 POST `/api/settings/cad`

- 源码：`src/routes/settings.rs:198`
- 请求体：`CadSettingsRequest`（字段见上文类型表）
- 响应顶层键：`code`、`status`
- `status` 取值：error / ok

### §47 POST `/api/settings/search-agent`

- 源码：`src/routes/settings.rs:169`
- 读取的请求键：`daily_budget`、`rerank_enabled`
- 响应顶层键：`code`、`daily_budget`、`rerank_enabled`、`status`
- `status` 取值：error / ok

### §48 POST `/api/settings/serpapi`

- 源码：`src/routes/settings.rs:472`
- 读取的请求键：`api_keys`、`epo_key`、`epo_secret`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §49 POST `/api/settings/ai`

- 源码：`src/routes/settings.rs:534`
- 读取的请求键：`ai_model_expert`、`api_key`、`base_url`、`gemini_cli_enabled`、`google_client_id`、`google_client_secret`、`model`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §50 GET `/api/settings/serpapi/balance`

- 源码：`src/routes/settings.rs:744`
- 响应顶层键：`key_count`、`message`、`plan_name`、`remaining`、`searches_per_month`、`status`、`this_month_usage`、`total_usage`
- `status` 取值：error / ok

### §51 GET `/api/chat/{session_key}`

- 源码：`src/routes/chat.rs:21`
- 响应顶层键：`limit`、`message`、`messages`、`offset`、`status`、`total`
- `status` 取值：error / ok

### §52 POST `/api/chat/{session_key}/save`

- 源码：`src/routes/chat.rs:53`
- 读取的请求键：`content`、`role`
- 响应顶层键：`message`、`message_id`、`status`
- `status` 取值：error / ok

### §53 POST `/api/chat/{session_key}/delete`

- 源码：`src/routes/chat.rs:90`
- 响应顶层键：`deleted`、`message`、`status`
- `status` 取值：error / ok

### §55 POST `/api/search/stats`

- 源码：`src/routes/search.rs:1466`
- 请求体：`SearchRequest`（字段见上文类型表）
- 响应顶层键：`applicants`、`countries`、`total`、`years`

### §58 POST `/api/search/online`

- 源码：`src/routes/search.rs:671`
- 请求体：`SearchRequest`（字段见上文类型表）
- 响应顶层键：`attempts`、`message`、`page`、`page_size`、`patents`、`source`、`total`

### §59 POST `/api/search/vector`

- 源码：`src/routes/search.rs:1305`
- 读取的请求键：`limit`、`query`、`rrf_k`
- 响应顶层键：`applicant`、`bm25_count`、`error`、`fused_score`、`id`、`inventor`、`method`、`patent_number`、`patents`、`relevance_score`、`rrf_k`、`section_tag`、`source`、`title`、`total`、`vector_count`

### §60 POST `/api/search/analyze`

- 源码：`src/routes/ai.rs:811`
- 读取的请求键：`patents`、`query`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §61 POST `/api/patent/fetch`

- 源码：`src/routes/patent.rs:14`
- 请求体：`FetchPatentRequest`（字段见上文类型表）
- 响应顶层键：`message`、`patent`、`status`
- `status` 取值：error / ok

### §62 GET `/api/patent/enrich/{id}`

- 源码：`src/routes/patent.rs:35`
- 响应顶层键：`message`、`patent`、`status`
- `status` 取值：error / ok

### §63 GET `/api/patent/enrich-free/{id}`

- 源码：`src/routes/patent.rs:247`
- 响应顶层键：`message`、`patent`、`remaining_secs`、`status`
- `status` 取值：cooldown / error / ok

### §65 POST `/api/patent/pdf/extract-text`

- 源码：`src/routes/upload.rs:1139`
- 响应顶层键：`char_count`、`error`、`full_text`、`method`、`page`、`page_count`、`pages`、`status`、`text`、`total_chars`
- `status` 取值：ok

### §66 GET `/api/patent/image-proxy`

- 源码：`src/routes/patent.rs:1002`
- 读取的查询键：`url`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §67 GET `/api/patent/lookup/{number}`

- 源码：`src/routes/patent.rs:282`
- 读取的查询键：`fetch`
- 响应顶层键：`message`、`patent`、`status`
- `status` 取值：error / ok

### §68 POST `/api/patent/lookup-or-fetch`

- 源码：`src/routes/patent.rs:453`
- 读取的请求键：`patent_number`
- 响应顶层键：`message`、`patent`、`source`、`status`
- `status` 取值：error / ok

### §69 POST `/api/patent/resolve-chinese-titles`

- 源码：`src/routes/patent.rs:363`
- 请求体：`ResolveTitlesRequest`（字段见上文类型表）
- 响应顶层键：`status`、`titles`
- `status` 取值：ok

### §70 GET `/api/patent/similar/{id}`

- 源码：`src/routes/patent.rs:535`
- 响应顶层键：`error`、`similar`

### §71 GET `/api/patent/{id}/legal-status`

- 源码：`src/routes/patent.rs:1122`
- 响应顶层键：`message`、`result`、`status`
- `status` 取值：error / ok

### §72 POST `/api/ai/chat`

- 源码：`src/routes/ai.rs:410`
- 请求体：`AiChatRequest`（字段见上文类型表）
- 响应顶层键：`content`、`image_url`、`messages`、`model`、`role`、`temperature`、`text`、`type`

### §74 POST `/api/ai/chat/conclusions`

- 源码：`src/routes/ai.rs:591`
- 读取的请求键：`history`、`patent_id`、`session_key`
- 响应顶层键：`conclusions`、`error`、`status`
- `status` 取值：ok

### §75 GET `/api/auth/google/url`

- 源码：`src/routes/auth.rs:387`
- 响应顶层键：`error`、`redirect_uri`、`url`

### §77 POST `/api/auth/google/exchange`

- 源码：`src/routes/auth.rs:447`
- 响应顶层键：`error`、`expires_in`、`has_refresh_token`、`message`、`status`
- `status` 取值：ok

### §78 GET `/api/auth/google/status`

- 源码：`src/routes/auth.rs:549`
- 响应顶层键：`auth_mode`、`client_id_set`、`client_secret_set`、`has_credentials`、`has_token`、`refresh_token_set`、`token_expired`、`token_expiry`

### §79 GET `/api/auth/gcloud/status`

- 源码：`src/routes/auth.rs:60`
- 响应顶层键：`adc_email`、`adc_file_exists`、`adc_file_path`、`adc_type`、`gcloud_available`、`has_token`、`is_gcloud_mode`、`token_expired`

### §80 POST `/api/auth/gcloud/login`

- 源码：`src/routes/auth.rs:116`
- 响应顶层键：`expires_in`、`message`、`method`、`status`
- `status` 取值：error / ok

### §82 POST `/api/ai/compare`

- 源码：`src/routes/ai.rs:745`
- 读取的请求键：`items`、`patent_id1`、`patent_id2`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §83 POST `/api/ai/claims`

- 源码：`src/routes/ai.rs:888`
- 读取的请求键：`patent_id`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §84 POST `/api/ai/risk`

- 源码：`src/routes/ai.rs:917`
- 读取的请求键：`patent_ids`、`product_description`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §85 POST `/api/ai/compare-matrix`

- 源码：`src/routes/ai.rs:980`
- 读取的请求键：`items`、`patent_ids`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §86 POST `/api/ai/batch-summarize`

- 源码：`src/routes/ai.rs:1066`
- 读取的请求键：`patent_ids`
- 响应顶层键：`error`、`id`、`patent_number`、`status`、`summaries`、`summary`、`title`
- `status` 取值：ok

### §87 POST `/api/ai/inventiveness-analysis`

- 源码：`src/routes/ai.rs:1220`
- 读取的请求键：`reference_ids`、`references`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §88 POST `/api/ai/office-action-response`

- 源码：`src/routes/ai.rs:1280`
- 读取的请求键：`depth`、`discuss`、`first_exam_context`、`force`、`my_patent`、`my_patent_id`、`oa_date`、`oa_type`、`office_action`、`references`、`strategy_hint`
- 响应顶层键：`analysis`、`cached`、`error`、`patent_number`、`patent_title`、`status`、`version`
- `status` 取值：ok

### §89 POST `/api/ai/office-action-response/stream`

- 源码：`src/routes/ai.rs:1571`
- 读取的请求键：`case_meta`、`depth`、`discuss`、`first_exam_context`、`my_patent`、`my_patent_id`、`oa_date`、`oa_type`、`office_action`、`references`、`strategy_hint`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §90 POST `/api/ai/oa-generate-response-letter`

- 源码：`src/routes/ai.rs:1894`
- 读取的请求键：`analysis`、`analysis_source`、`case_meta`、`current_claims`、`discussion`、`discussion_id`、`my_patent`、`oa_date`、`oa_full_text`、`oa_type`、`office_action`
- 响应顶层键：未抽取到（动态构造或非 JSON 响应）

### §91 POST `/api/ai/oa-discuss`

- 源码：`src/routes/ai.rs:2197`
- 读取的请求键：`analysis`、`applicant_name`、`discussion`、`discussion_id`、`file_base64`、`file_name`、`file_type`、`message`、`oa_text`、`oa_type`、`office_action`、`patent_number`
- 响应顶层键：`content`、`role`

### §92 POST `/api/ai/oa-fetch-refs`

- 源码：`src/routes/ai.rs:2878`
- 读取的请求键：`oa_text`
- 响应顶层键：`abstract`、`context`、`found`、`found_count`、`full_text`、`message`、`pub_number`、`refs`、`source`、`status`、`title`、`total`
- `status` 取值：error / ok

### §93 POST `/api/ai/oa-panel`

- 源码：`src/routes/ai.rs:2966`
- 读取的请求键：`my_patent`、`oa_text`、`refs`
- 响应顶层键：`chair`、`error`、`rebuttals`、`roles`、`status`、`synthesis`
- `status` 取值：ok

### §94 POST `/api/ai/oa-history-compare`

- 源码：`src/routes/ai.rs:3147`
- 读取的请求键：`current_oa_text`、`current_oa_type`、`patent_id`
- 响应顶层键：`analysis`、`error`、`history_count`、`message`、`status`
- `status` 取值：no_history / ok

### §95 POST `/api/ai/oa-examiner-preview`

- 源码：`src/routes/ai.rs:3215`
- 读取的请求键：`my_patent`、`oa_text`、`response_text`
- 响应顶层键：`error`、`preview`、`status`
- `status` 取值：ok

### §96 POST `/api/ai/oa-defense-analysis`

- 源码：`src/routes/ai.rs:3262`
- 读取的请求键：`claims`、`my_patent`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §97 POST `/api/ai/oa-strategy-recommend`

- 源码：`src/routes/ai.rs:3317`
- 读取的请求键：`my_patent`、`oa_text`、`oa_type`、`refs`
- 响应顶层键：`error`、`status`、`strategies`
- `status` 取值：ok

### §98 POST `/api/ai/oa-quality-score`

- 源码：`src/routes/ai.rs:3355`
- 读取的请求键：`my_patent`、`oa_text`、`response_text`
- 响应顶层键：`error`
- `status` 取值：ok

### §99 POST `/api/ai/audit-checks`

- 源码：`src/routes/audit.rs:16`
- 读取的请求键：`ai_output`、`documents`、`golden_output`、`key_terms`、`known_features`、`oa_text`、`pdf_text`、`submitted_claim_count`、`submitted_claims`
- 响应顶层键：`actual_sources`、`char_diff_count`、`citations`、`claimed_source`、`concluding_paragraph`、`count`、`covered_features`、`dependent_claim_evaluations`、`distinction_features`、`doc_id`、`entries`、`entry_count`、`error`、`evaluated_claims`、`feature`、`feature_coverage`、`features`、`filing_date`、`filing_info`、`filing_serial`、`found_in_original`、`involved_claims`、`ipc`、`is_correct`、`is_reliable`、`match_position`、`method`、`misattribution_count`、`mismatch_count`、`note`、`paraphrase_text`、`passed`、`publication_number`、`quoted_text`、`rejected_claims`、`relevance_type`、`response_deadline_months`、`response_deadline_text`、`results`、`risk_warning`、`rule_description`、`rule_id`、`segment_count`、`segments`、`severity`、`similarity`、`skipped`、`status`、`summary`、`term_coverage_diff`、`text_length`、`uncovered_features`、`unevaluated_claims`、`violating_text`、`violations`
- `status` 取值：available / ok

### §100 POST `/api/ai/prohibition-check`

- 源码：`src/routes/audit.rs:257`
- 读取的请求键：`response_text`
- 响应顶层键：`count`、`error`、`rule_description`、`rule_id`、`severity`、`status`、`summary`、`violating_text`、`violations`
- `status` 取值：ok

### §101 POST `/api/ai/oa-claim-simulate`

- 源码：`src/routes/ai.rs:3516`
- 读取的请求键：`modified_claims`、`oa_text`、`original_claims`、`refs`
- 响应顶层键：`error`、`raw`、`simulation`、`status`
- `status` 取值：ok

### §102 POST `/api/ai/oa-collect-evidence`

- 源码：`src/routes/ai.rs:3667`
- 读取的请求键：`my_patent`、`oa_text`、`refs`
- 响应顶层键：`evidence`、`message`、`stats`、`status`、`summary`
- `status` 取值：error / ok

### §103 POST `/api/ai/oa-adversarial`

- 源码：`src/routes/ai.rs:3739`
- 读取的请求键：`claims`、`focus`、`goal_threshold`、`my_patent`、`oa_text`、`refs`、`rounds`
- 响应顶层键：`confidence_score`、`converged`、`error`、`final_response`、`goal_achieved`、`goal_threshold`、`rounds_completed`、`status`、`transcript`、`weak_points`
- `status` 取值：ok

### §104 GET `/api/oa/history/all`

- 源码：`src/routes/ai.rs:2537`
- 响应顶层键：`analyses`、`message`、`status`
- `status` 取值：error / ok

### §105 GET `/api/oa/history/{patent_number}`

- 源码：`src/routes/ai.rs:2526`
- 响应顶层键：`analyses`、`message`、`status`
- `status` 取值：error / ok

### §106 GET `/api/oa/history/detail/{id}`

- 源码：`src/routes/ai.rs:2548`
- 响应顶层键：`analysis`、`message`、`status`
- `status` 取值：error / ok

### §107 POST `/api/oa/history/{id}/delete`

- 源码：`src/routes/ai.rs:2656`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §108 GET `/api/oa/history/{patent_number}/diff`

- 源码：`src/routes/oa_diff.rs:21`
- 响应顶层键：`added_refs`、`analyses`、`analysis_diff`、`analysis_full`、`analysis_preview`、`created_at`、`depth`、`depth_changed`、`diffs`、`first_exam_context`、`from_analysis_preview`、`from_claim_changes`、`from_date`、`from_depth`、`from_oa_type`、`from_version`、`history`、`message`、`new_reasons`、`oa_type`、`oa_type_changed`、`overcome_reasons`、`patent_number`、`patent_title`、`ref_diff`、`removed_refs`、`status`、`to_analysis_preview`、`to_claim_changes`、`to_date`、`to_depth`、`to_oa_type`、`to_version`、`total_rounds`、`version`
- `status` 取值：error / ok

### §109 GET `/api/oa/discussions/{patent_number}`

- 源码：`src/routes/ai.rs:2560`
- 响应顶层键：`discussions`、`message`、`status`
- `status` 取值：error / ok

### §110 GET `/api/oa/discussions/{patent_number}/{discussion_id}`

- 源码：`src/routes/ai.rs:2571`
- 响应顶层键：`discussion`、`message`、`status`
- `status` 取值：error / ok

### §111 POST `/api/oa/discussions/import`

- 源码：`src/routes/ai.rs:2593`
- 读取的请求键：`messages`、`oa_text`、`patent_number`
- 响应顶层键：`content`、`discussion_id`、`message`、`ok`、`role`、`status`
- `status` 取值：error / ok

### §112 POST `/api/oa/export-docx`

- 源码：`src/routes/ai.rs:2773`
- 读取的请求键：`applicant`、`oa_type`、`patent_number`、`response_text`
- 响应顶层键：`docx`、`error`、`status`
- `status` 取值：ok

### §115 GET `/api/oa/rounds/{patent_id}`

- 源码：`src/routes/oa_rounds.rs:15`
- 响应顶层键：`count`、`message`、`patent_id`、`rounds`、`status`
- `status` 取值：error / ok

### §116 POST `/api/oa/rounds`

- 源码：`src/routes/oa_rounds.rs:41`
- 读取的请求键：`notes`、`oa_date`、`oa_text`、`patent_id`、`quality_score`、`response_date`、`response_text`、`result`、`result_date`、`round_type`、`strategy_used`
- 响应顶层键：`message`、`round`、`status`
- `status` 取值：error / ok

### §117 PUT `/api/oa/rounds/update/{id}`

- 源码：`src/routes/oa_rounds.rs:156`
- 读取的请求键：`notes`、`oa_date`、`oa_text`、`quality_score`、`response_date`、`response_text`、`result`、`result_date`、`strategy_used`
- 响应顶层键：`message`、`round`、`status`
- `status` 取值：error / ok

### §118 GET `/api/oa/timeline/{patent_id}`

- 源码：`src/routes/oa_rounds.rs:231`
- 响应顶层键：`analyses`、`has_response`、`has_result`、`patent_id`、`round`、`status`、`summary`、`timeline`
- `status` 取值：ok

### §119 POST `/api/ai/check-amendments`

- 源码：`src/routes/ai.rs:2667`
- 读取的请求键：`amended_claims`、`office_action`、`original_claims`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §120 POST `/api/ai/threat-assessment`

- 源码：`src/routes/ai.rs:2704`
- 读取的请求键：`my_claims`、`patents`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §121 POST `/api/ai/claim-chart`

- 源码：`src/routes/ai.rs:2735`
- 读取的请求键：`my_claims`、`prior_art`
- 响应顶层键：`analysis`、`error`、`status`
- `status` 取值：ok

### §122 GET `/api/ai/cost`

- 源码：`src/routes/ai.rs:2810`
- 响应顶层键：`error`

### §123 GET `/api/ai/cost/records`

- 源码：`src/routes/ai.rs:2821`
- 响应顶层键：`count`、`error`、`records`、`status`
- `status` 取值：ok

### §124 POST `/api/ai/cost/record`

- 源码：`src/routes/ai.rs:2836`
- 读取的请求键：`duration_ms`、`estimated_cost_cents`、`input_tokens`、`model`、`output_tokens`、`pipeline_run_id`、`provider`、`step`
- 响应顶层键：`error`、`id`、`status`
- `status` 取值：ok

### §125 POST `/api/idea/submit`

- 源码：`src/routes/idea.rs:171`
- 请求体：`IdeaSubmitRequest`（字段见上文类型表）
- 响应顶层键：`id`、`message`、`status`
- `status` 取值：error / ok

### §126 POST `/api/idea/analyze`

- 源码：`src/routes/idea.rs:219`
- 读取的请求键：`id`
- 响应顶层键：`abstract`、`idea`、`link`、`message`、`patent_number`、`snippet`、`source`、`status`、`title`
- `status` 取值：error / ok

### §127 POST `/api/idea/pipeline`

- 源码：`src/routes/idea.rs:997`
- 读取的请求键：`id`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §128 POST `/api/ideas/batch-compare`

- 源码：`src/routes/idea.rs:1855`
- 读取的请求键：`idea_ids`
- 响应顶层键：`description`、`id`、`ideas`、`message`、`novelty_score`、`similarity_matrix`、`status`、`title`
- `status` 取值：error / ok

### §129 GET `/api/idea/list`

- 源码：`src/routes/idea.rs:399`
- 响应顶层键：`ideas`、`message`、`status`
- `status` 取值：error / ok

### §130 GET `/api/idea/{id}`

- 源码：`src/routes/idea.rs:367`
- 响应顶层键：`idea`、`message`、`status`
- `status` 取值：error / ok

### §131 POST `/api/idea/{id}/delete`

- 源码：`src/routes/idea.rs:409`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §133 POST `/api/idea/{id}/resume`

- 源码：`src/routes/idea.rs:1156`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §134 GET / POST `/api/idea/{id}/research-state`

- **GET** `routes::api_idea_research_state`（`src/routes/idea.rs:2068`）
  - 响应顶层键：`message`、`research_state`、`source`、`status`
  - `status` 取值：error / ok
- **POST** `routes::api_idea_research_state_update`（`src/routes/idea.rs:2087`）
  - 请求体：`ResearchStateUpdateRequest`（字段见上文类型表）
  - 响应顶层键：`message`、`research_state`、`status`
  - `status` 取值：error / ok

### §135 POST `/api/idea/{id}/redirect`

- 源码：`src/routes/idea.rs:2108`
- 请求体：`RedirectRequest`（字段见上文类型表）
- 响应顶层键：`message`、`reason`、`restart_from`、`status`
- `status` 取值：error / ok

### §136 GET `/api/idea/{id}/report`

- 源码：`src/routes/idea.rs:1267`
- 读取的查询键：`type`、`version_id`
- 响应顶层键：`message`、`patent_results`、`report`、`report_type`、`status`、`web_results`
- `status` 取值：error / ok

### §138 GET `/api/ai/models`

- 源码：`src/routes/idea.rs:2474`
- 读取的查询键：`provider`
- 响应顶层键：`base_url`、`configured`、`count`、`error`、`id`、`models`、`name`、`provider`、`providers`、`status`、`total_count`
- `status` 取值：error / not_configured / not_found / ok / timeout

### §139 POST `/api/idea/{id}/chat`

- 源码：`src/routes/idea.rs:546`
- 请求体：`IdeaChatRequest`（字段见上文类型表）
- 响应顶层键：`content`、`error`、`image_url`、`message`、`messages`、`model`、`role`、`status`、`temperature`、`text`、`type`
- `status` 取值：ok

### §140 GET `/api/idea/{id}/evidence`

- 源码：`src/routes/idea.rs:438`
- 响应顶层键：`count`、`evidence`、`message`、`status`
- `status` 取值：error / ok

### §141 GET `/api/idea/{id}/kernel-report`

- 源码：`src/routes/idea.rs:455`
- 响应顶层键：`current_step`、`kernel_report`、`message`、`status`
- `status` 取值：error / no_kernel_report / no_snapshot / ok

### §142 GET `/api/idea/{id}/messages`

- 源码：`src/routes/idea.rs:971`
- 响应顶层键：`content`、`created_at`、`error`、`id`、`messages`、`role`

### §143 GET `/api/idea/{id}/chat/conclusions`

- 源码：`src/routes/idea.rs:1730`
- 响应顶层键：`conclusions`、`error`、`status`
- `status` 取值：ok

### §144 GET `/api/idea/{id}/chat/conversation`

- 源码：`src/routes/idea.rs:1709`
- 响应顶层键：`content`、`created_at`、`error`、`idea`、`messages`、`role`、`status`
- `status` 取值：ok

### §145 POST `/api/idea/{id}/summarize`

- 源码：`src/routes/idea.rs:1798`
- 响应顶层键：`error`、`status`、`summary`
- `status` 取值：ok

### §146 GET / POST `/api/ideas/{id}/feature-cards`

- **GET** `routes::api_get_feature_cards`（`src/routes/feature_cards.rs:16`）
  - 响应顶层键：`cards`、`message`、`status`
  - `status` 取值：error / ok
- **POST** `routes::api_create_feature_card`（`src/routes/feature_cards.rs:50`）
  - 请求体：`CreateFeatureCardRequest`（字段见上文类型表）
  - 响应顶层键：`card`、`message`、`status`
  - `status` 取值：error / ok

### §147 GET `/api/feature-cards/diff`

- 源码：`src/routes/feature_cards.rs:121`
- 读取的查询键：`a`、`b`
- 响应顶层键：`card_a`、`card_b`、`diff`、`diff_type`、`message`、`novelty_reason`、`novelty_significance`、`status`
- `status` 取值：error / ok

### §148 GET `/api/idea/{id}/claim-tree`

- 源码：`src/routes/idea.rs:2221`
- 响应顶层键：`claim_number`、`claim_type`、`claims`、`content`、`created_at`、`description`、`id`、`message`、`novelty_flag`、`parent_claim_id`、`status`
- `status` 取值：error / ok

### §149 POST `/api/idea/{id}/iterate`

- 源码：`src/routes/idea.rs:1955`
- 响应顶层键：`iteration`、`message`、`status`
- `status` 取值：error / ok

### §150 GET `/api/idea/{id}/versions`

- 源码：`src/routes/idea.rs:2290`
- 响应顶层键：`branch_id`、`created_at`、`current_step`、`id`、`message`、`status`、`version_number`、`versions`
- `status` 取值：error / ok

### §151 GET `/api/idea/{id}/branches`

- 源码：`src/routes/idea.rs:2315`
- 响应顶层键：`branches`、`message`、`status`
- `status` 取值：error / ok

### §152 GET `/api/idea/{id}/findings`

- 源码：`src/routes/idea.rs:2326`
- 响应顶层键：`findings`、`message`、`status`
- `status` 取值：error / ok

### §153 GET / POST `/api/idea/{id}/memory`

- **GET** `routes::api_idea_memory`（`src/routes/idea.rs:2337`）
  - 响应顶层键：`by_type`、`counts`、`entries`、`message`、`status`、`total`
  - `status` 取值：error / ok
- **POST** `routes::api_idea_memory_add`（`src/routes/idea.rs:2358`）
  - 读取的请求键：`concept_name`、`concept_type`、`confidence`、`content`
  - 响应顶层键：`id`、`message`、`status`
  - `status` 取值：error / ok

### §154 DELETE `/api/idea/{id}/memory/{entry_id}`

- 源码：`src/routes/idea.rs:2416`
- 响应顶层键：`message`、`status`
- `status` 取值：error / ok

### §155 GET `/api/ipc/tree`

- 源码：`src/routes/ipc.rs:22`
- 响应顶层键：`classes`、`code`、`count`、`message`、`name`、`sections`、`status`、`subclasses`、`total_codes`
- `status` 取值：error / ok

### §156 GET `/api/ipc/{code}/patents`

- 源码：`src/routes/ipc.rs:122`
- 响应顶层键：`code`、`count`、`message`、`patents`、`status`
- `status` 取值：error / ok

### §157 POST `/api/patents/import`

- 源码：`src/routes/settings.rs:802`
- 请求体：`ImportRequest`（字段见上文类型表）
- 响应顶层键：`imported`、`status`
- `status` 取值：ok

### §158 GET / POST `/api/collections`

- **GET** `routes::api_list_collections`（`src/routes/collections.rs:28`）
  - 响应顶层键：`collections`、`created_at`、`description`、`error`、`id`、`name`、`patent_count`
- **POST** `routes::api_create_collection`（`src/routes/collections.rs:8`）
  - 读取的请求键：`description`、`name`
  - 响应顶层键：`error`、`id`、`name`、`status`
  - `status` 取值：ok

### §159 DELETE `/api/collections/{id}`

- 源码：`src/routes/collections.rs:49`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §160 GET `/api/collections/{id}/patents`

- 源码：`src/routes/collections.rs:92`
- 响应顶层键：`error`、`patents`

### §161 POST `/api/collections/{id}/add`

- 源码：`src/routes/collections.rs:59`
- 读取的请求键：`patent_id`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §162 DELETE `/api/collections/{id}/remove/{patent_id}`

- 源码：`src/routes/collections.rs:82`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §163 GET / POST `/api/patents/{id}/tags`

- **GET** `routes::api_get_patent_tags`（`src/routes/collections.rs:128`）
  - 响应顶层键：`error`、`tags`
- **POST** `routes::api_add_tag`（`src/routes/collections.rs:102`）
  - 读取的请求键：`tag`
  - 响应顶层键：`error`、`status`
  - `status` 取值：ok

### §164 DELETE `/api/patents/{id}/tags/{tag}`

- 源码：`src/routes/collections.rs:118`
- 响应顶层键：`error`、`status`
- `status` 取值：ok

### §165 GET `/api/patents/{id}/collections`

- 源码：`src/routes/collections.rs:151`
- 响应顶层键：`collection_ids`、`error`

### §166 GET `/api/tags`

- 源码：`src/routes/collections.rs:138`
- 响应顶层键：`count`、`error`、`tag`、`tags`

### §167 POST `/api/upload/compare`

- 源码：`src/routes/upload.rs:192`
- 响应顶层键：`analysis`、`content_length`、`error`、`file_type`、`success`

### §168 POST `/api/upload/extract`

- 源码：`src/routes/upload.rs:370`
- 响应顶层键：`char_length`、`error`、`file_type`、`length`、`text`、`truncated`

### §169 POST `/api/upload/pdf-store`

- 源码：`src/routes/upload.rs:127`
- 响应顶层键：`filename`、`message`、`size`、`status`、`url`
- `status` 取值：error / ok

## 已知缺口

- 1 条任意 JSON 端点**未抽取到请求键**（处理器可能完全不用请求体字段，或经共享 helper/嵌套结构读取）。
- 响应只列顶层键：嵌套对象的内部键、数组元素结构、以及动态拼接的键未展开。
- 不覆盖：鉴权要求、速率限制、错误码语义。这些属于运行时行为，见 `docs/API.md` 与对应源码。
