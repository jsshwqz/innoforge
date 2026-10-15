# 路由清单（重构防丢失基准）

> 生成方式：从 src/common.rs::build_router 机械提取（2026-08-15，HEAD=4875ffd 前后）。**118 条 route 注册 / 120 个方法绑定**（2 条双方法）。T3.4 的路由数量断言测试以本表为基准；任何增删路由必须先改本表并说明理由。

## _pages（9）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/` | index_page |
| GET | `/ai` | ai_page |
| GET | `/compare` | compare_page |
| GET | `/idea` | idea_page |
| GET | `/oa-response` | office_action_response_page |
| GET | `/patent/:id` | patent_detail_page |
| GET | `/search` | search_page |
| GET | `/settings` | settings_page |
| GET | `/static/*path` | serve_static_embedded |

## api/ai（21）

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/api/ai/batch-summarize` | api_ai_batch_summarize |
| POST | `/api/ai/chat` | api_ai_chat |
| POST | `/api/ai/chat/conclusions` | api_ai_chat_conclusions |
| POST | `/api/ai/chat/stream` | api_ai_chat_stream |
| POST | `/api/ai/check-amendments` | api_ai_check_amendments |
| POST | `/api/ai/claim-chart` | api_ai_claim_chart |
| POST | `/api/ai/claims` | api_ai_claims_analysis |
| POST | `/api/ai/compare` | api_ai_compare |
| POST | `/api/ai/compare-matrix` | api_ai_compare_matrix |
| GET | `/api/ai/cost` | api_ai_cost_summary |
| POST | `/api/ai/cost/record` | api_ai_cost_save |
| GET | `/api/ai/cost/records` | api_ai_cost_records |
| POST | `/api/ai/inventiveness-analysis` | api_ai_inventiveness_analysis |
| GET | `/api/ai/models` | list_ai_models |
| POST | `/api/ai/oa-discuss` | api_ai_oa_discuss |
| POST | `/api/ai/oa-generate-response-letter` | api_ai_oa_generate_response_letter |
| POST | `/api/ai/office-action-response` | api_ai_office_action_response |
| POST | `/api/ai/office-action-response/stream` | api_ai_office_action_response_stream |
| POST | `/api/ai/risk` | api_ai_risk_assessment |
| POST | `/api/ai/summarize` | api_ai_summarize |
| POST | `/api/ai/threat-assessment` | api_ai_threat_assessment |

## api/auth（6）

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/api/auth/gcloud/login` | api_gcloud_login |
| GET | `/api/auth/gcloud/status` | api_gcloud_status |
| GET | `/api/auth/google/callback` | api_google_callback |
| POST | `/api/auth/google/exchange` | api_google_exchange_handler |
| GET | `/api/auth/google/status` | api_google_oauth_status |
| GET | `/api/auth/google/url` | api_google_auth_url |

## api/cad（6）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/cad/artifacts` | api_cad_artifacts |
| GET | `/api/cad/artifacts/:id/download/:format` | api_cad_download |
| GET | `/api/cad/artifacts/:id/preview` | api_cad_preview |
| POST | `/api/cad/draw` | api_cad_draw |
| POST | `/api/cad/start` | api_cad_start |
| GET | `/api/cad/status` | api_cad_status |

## api/chat（3）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/chat/:session_key` | api_chat_get_messages |
| POST | `/api/chat/:session_key/delete` | api_chat_delete_messages |
| POST | `/api/chat/:session_key/save` | api_chat_save_message |

## api/collections（4）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/collections` | api_list_collections |
| POST | `/api/collections` | api_create_collection |
| POST | `/api/collections/:id/add` | api_add_to_collection |
| GET | `/api/collections/:id/patents` | api_get_collection_patents |

## api/feature-cards（1）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/feature-cards/diff` | api_feature_card_diff |

## api/idea（27）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/idea/:id` | api_idea_get |
| GET | `/api/idea/:id/branches` | api_idea_branches |
| POST | `/api/idea/:id/chat` | api_idea_chat |
| GET | `/api/idea/:id/chat/conclusions` | api_idea_chat_conclusions |
| GET | `/api/idea/:id/chat/conversation` | api_idea_chat_conversation |
| GET | `/api/idea/:id/claim-tree` | api_idea_claim_tree |
| POST | `/api/idea/:id/delete` | api_idea_delete |
| GET | `/api/idea/:id/evidence` | api_idea_evidence |
| GET | `/api/idea/:id/findings` | api_idea_findings |
| POST | `/api/idea/:id/iterate` | api_idea_iterate |
| GET | `/api/idea/:id/memory` | api_idea_memory |
| POST | `/api/idea/:id/memory` | api_idea_memory_add |
| DELETE | `/api/idea/:id/memory/:entry_id` | api_idea_memory_delete |
| GET | `/api/idea/:id/messages` | api_idea_messages |
| GET | `/api/idea/:id/progress` | api_idea_progress |
| POST | `/api/idea/:id/redirect` | api_idea_redirect |
| GET | `/api/idea/:id/report` | api_idea_report |
| GET | `/api/idea/:id/report.html` | api_idea_report_html |
| POST | `/api/idea/:id/research-state` | api_idea_research_state_update |
| GET | `/api/idea/:id/research-state` | api_idea_research_state |
| POST | `/api/idea/:id/resume` | api_idea_resume |
| POST | `/api/idea/:id/summarize` | api_idea_summarize_discussion |
| GET | `/api/idea/:id/versions` | api_idea_versions |
| POST | `/api/idea/analyze` | api_idea_analyze |
| GET | `/api/idea/list` | api_idea_list |
| POST | `/api/idea/pipeline` | api_idea_pipeline |
| POST | `/api/idea/submit` | api_idea_submit |

## api/ideas（3）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/ideas/:id/feature-cards` | api_get_feature_cards |
| POST | `/api/ideas/:id/feature-cards` | api_create_feature_card |
| POST | `/api/ideas/batch-compare` | api_ideas_batch_compare |

## api/ipc（2）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/ipc/:code/patents` | api_ipc_patents |
| GET | `/api/ipc/tree` | api_ipc_tree |

## api/oa（8）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/oa/discussions/:patent_number` | api_oa_discussion_list |
| GET | `/api/oa/discussions/:patent_number/:discussion_id` | api_oa_discussion_get |
| POST | `/api/oa/discussions/import` | api_oa_discussion_import |
| POST | `/api/oa/export-docx` | api_oa_export_docx |
| POST | `/api/oa/history/:id/delete` | api_oa_history_delete |
| GET | `/api/oa/history/:patent_number` | api_oa_history |
| GET | `/api/oa/history/all` | api_oa_history_all |
| GET | `/api/oa/history/detail/:id` | api_oa_history_all |

## api/patent（10）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/patent/:id/legal-status` | api_patent_legal_status |
| GET | `/api/patent/enrich-free/:id` | api_enrich_patent_free |
| GET | `/api/patent/enrich/:id` | api_enrich_patent |
| POST | `/api/patent/fetch` | api_fetch_patent |
| GET | `/api/patent/image-proxy` | api_patent_image_proxy |
| POST | `/api/patent/lookup-or-fetch` | api_patent_lookup_and_fetch |
| GET | `/api/patent/lookup/:number` | api_patent_lookup |
| GET | `/api/patent/pdf/:id` | api_patent_pdf |
| POST | `/api/patent/pdf/extract-text` | api_patent_pdf_extract_text |
| GET | `/api/patent/similar/:id` | api_recommend_similar |

## api/patents（4）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/patents/:id/collections` | api_get_patent_collections |
| GET | `/api/patents/:id/tags` | api_get_patent_tags |
| POST | `/api/patents/:id/tags` | api_add_tag |
| POST | `/api/patents/import` | api_import_patents |

## api/search（7）

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/api/search` | api_search |
| POST | `/api/search/analyze` | api_ai_analyze_results |
| POST | `/api/search/export` | api_export_csv |
| POST | `/api/search/export/xlsx` | api_export_xlsx |
| POST | `/api/search/online` | api_search_online |
| POST | `/api/search/stats` | api_search_stats |
| POST | `/api/search/vector` | api_search_vector |

## api/settings（5）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/settings` | api_get_settings |
| POST | `/api/settings/ai` | api_save_ai |
| POST | `/api/settings/cad` | api_save_cad_settings |
| POST | `/api/settings/serpapi` | api_save_serpapi |
| GET | `/api/settings/serpapi/balance` | api_serpapi_balance |

## api/tags（1）

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/api/tags` | api_list_all_tags |

## api/upload（3）

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/api/upload/compare` | api_upload_compare |
| POST | `/api/upload/extract` | api_upload_extract |
| POST | `/api/upload/pdf-store` | api_upload_pdf_store |

## 非 API 特殊路由

- `nest_service("/uploads", ServeDir::new("data/uploads"))` — 上传文件静态服务
- `/static/*path` → serve_static_embedded（rust-embed 编译期资源）
- 页面路由 8 条见 _pages 分组

## 全局中间件（T3.0 迁移时必须原样保留）

1. DefaultBodyLimit::max(20MB)；2. CORS 白名单（默认本机 3000 + INNOFORGE_CORS_ORIGINS 严格校验）；3. X-Frame-Options: DENY；4. X-Content-Type-Options: nosniff；5. Referrer-Policy: strict-origin-when-cross-origin
