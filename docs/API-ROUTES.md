# 路由清单（自动生成）

> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。
> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：
> `node scripts/gen_route_inventory.js`
> 逐条「收什么、回什么」的契约（路径参数 / 请求体字段 / 响应键）见 `docs/API-CONTRACTS.md`，同样自动生成。

共 **143** 条路由：页面/静态 9 条，`/api` 接口 134 条。

## 说明：清单只回答「有哪些端点」

清单保证「有哪些端点、处理函数是哪个、在 common.rs 第几行」；字段级契约在 `docs/API-CONTRACTS.md`；
叙述性说明、示例与业务语义在 `docs/API.md`。三份文件分工不同，任何一份都不假装自己覆盖了另外两份。

| # | 方法 | 路径 | 处理函数 | 源码位置 |
| --- | --- | --- | --- | --- |
| 1 | GET | `/` | `routes::index_page` | src/common.rs:381 |
| 2 | GET | `/search` | `routes::search_page` | src/common.rs:382 |
| 3 | GET | `/patent/:id` | `routes::patent_detail_page` | src/common.rs:383 |
| 4 | GET | `/ai` | `routes::ai_page` | src/common.rs:384 |
| 5 | GET | `/compare` | `routes::compare_page` | src/common.rs:385 |
| 6 | GET | `/idea` | `routes::idea_page` | src/common.rs:386 |
| 7 | GET | `/settings` | `routes::settings_page` | src/common.rs:387 |
| 8 | GET | `/oa-response` | `routes::office_action_response_page` | src/common.rs:388 |
| 9 | GET | `/api/cad/status` | `routes::api_cad_status` | src/common.rs:390 |
| 10 | POST | `/api/cad/start` | `routes::api_cad_start` | src/common.rs:391 |
| 11 | POST | `/api/cad/draw` | `routes::api_cad_draw` | src/common.rs:392 |
| 12 | GET | `/api/cad/artifacts` | `routes::api_cad_artifacts` | src/common.rs:393 |
| 13 | GET | `/api/cad/artifacts/:id/preview` | `routes::api_cad_preview` | src/common.rs:394 |
| 14 | GET | `/api/cad/artifacts/:id/download/:format` | `routes::api_cad_download` | src/common.rs:398 |
| 15 | GET | `/api/version` | `routes::api_get_version` | src/common.rs:403 |
| 16 | GET | `/api/oa/deadline-table` | `crate::oa_deadline::deadline_table` | src/common.rs:405 |
| 17 | GET | `/api/oa/compute-deadline` | `crate::oa_deadline::compute_deadline_api` | src/common.rs:409 |
| 18 | GET | `/api/settings` | `routes::api_get_settings` | src/common.rs:413 |
| 19 | POST | `/api/settings/cad` | `routes::api_save_cad_settings` | src/common.rs:414 |
| 20 | POST | `/api/settings/search-agent` | `routes::api_save_search_agent_settings` | src/common.rs:415 |
| 21 | POST | `/api/settings/serpapi` | `routes::api_save_serpapi` | src/common.rs:419 |
| 22 | POST | `/api/settings/ai` | `routes::api_save_ai` | src/common.rs:420 |
| 23 | GET | `/api/settings/serpapi/balance` | `routes::api_serpapi_balance` | src/common.rs:421 |
| 24 | GET | `/api/chat/:session_key` | `routes::api_chat_get_messages` | src/common.rs:426 |
| 25 | POST | `/api/chat/:session_key/save` | `routes::api_chat_save_message` | src/common.rs:427 |
| 26 | POST | `/api/chat/:session_key/delete` | `routes::api_chat_delete_messages` | src/common.rs:431 |
| 27 | POST | `/api/search` | `routes::api_search` | src/common.rs:436 |
| 28 | POST | `/api/search/stats` | `routes::api_search_stats` | src/common.rs:437 |
| 29 | POST | `/api/search/export` | `routes::api_export_csv` | src/common.rs:438 |
| 30 | POST | `/api/search/export/xlsx` | `routes::api_export_xlsx` | src/common.rs:439 |
| 31 | POST | `/api/search/online` | `routes::api_search_online` | src/common.rs:440 |
| 32 | POST | `/api/search/vector` | `routes::api_search_vector` | src/common.rs:441 |
| 33 | POST | `/api/search/analyze` | `routes::api_ai_analyze_results` | src/common.rs:442 |
| 34 | POST | `/api/patent/fetch` | `routes::api_fetch_patent` | src/common.rs:444 |
| 35 | GET | `/api/patent/enrich/:id` | `routes::api_enrich_patent` | src/common.rs:445 |
| 36 | GET | `/api/patent/enrich-free/:id` | `routes::api_enrich_patent_free` | src/common.rs:446 |
| 37 | GET | `/api/patent/pdf/:id` | `routes::api_patent_pdf` | src/common.rs:450 |
| 38 | POST | `/api/patent/pdf/extract-text` | `routes::api_patent_pdf_extract_text` | src/common.rs:451 |
| 39 | GET | `/api/patent/image-proxy` | `routes::api_patent_image_proxy` | src/common.rs:455 |
| 40 | GET | `/api/patent/lookup/:number` | `routes::api_patent_lookup` | src/common.rs:459 |
| 41 | POST | `/api/patent/lookup-or-fetch` | `routes::api_patent_lookup_and_fetch` | src/common.rs:460 |
| 42 | POST | `/api/patent/resolve-chinese-titles` | `routes::api_resolve_chinese_titles` | src/common.rs:464 |
| 43 | GET | `/api/patent/similar/:id` | `routes::api_recommend_similar` | src/common.rs:468 |
| 44 | GET | `/api/patent/:id/legal-status` | `routes::api_patent_legal_status` | src/common.rs:472 |
| 45 | POST | `/api/ai/chat` | `routes::api_ai_chat` | src/common.rs:477 |
| 46 | POST | `/api/ai/chat/stream` | `routes::api_ai_chat_stream` | src/common.rs:478 |
| 47 | POST | `/api/ai/chat/conclusions` | `routes::api_ai_chat_conclusions` | src/common.rs:479 |
| 48 | GET | `/api/auth/google/url` | `routes::api_google_auth_url` | src/common.rs:484 |
| 49 | GET | `/api/auth/google/callback` | `routes::api_google_callback` | src/common.rs:485 |
| 50 | POST | `/api/auth/google/exchange` | `routes::api_google_exchange_handler` | src/common.rs:489 |
| 51 | GET | `/api/auth/google/status` | `routes::api_google_oauth_status` | src/common.rs:493 |
| 52 | GET | `/api/auth/gcloud/status` | `routes::api_gcloud_status` | src/common.rs:498 |
| 53 | POST | `/api/auth/gcloud/login` | `routes::api_gcloud_login` | src/common.rs:499 |
| 54 | POST | `/api/ai/summarize` | `routes::api_ai_summarize` | src/common.rs:500 |
| 55 | POST | `/api/ai/compare` | `routes::api_ai_compare` | src/common.rs:501 |
| 56 | POST | `/api/ai/claims` | `routes::api_ai_claims_analysis` | src/common.rs:502 |
| 57 | POST | `/api/ai/risk` | `routes::api_ai_risk_assessment` | src/common.rs:503 |
| 58 | POST | `/api/ai/compare-matrix` | `routes::api_ai_compare_matrix` | src/common.rs:504 |
| 59 | POST | `/api/ai/batch-summarize` | `routes::api_ai_batch_summarize` | src/common.rs:508 |
| 60 | POST | `/api/ai/inventiveness-analysis` | `routes::api_ai_inventiveness_analysis` | src/common.rs:512 |
| 61 | POST | `/api/ai/office-action-response` | `routes::api_ai_office_action_response` | src/common.rs:516 |
| 62 | POST | `/api/ai/office-action-response/stream` | `routes::api_ai_office_action_response_stream` | src/common.rs:520 |
| 63 | POST | `/api/ai/oa-generate-response-letter` | `routes::api_ai_oa_generate_response_letter` | src/common.rs:524 |
| 64 | POST | `/api/ai/oa-discuss` | `routes::api_ai_oa_discuss` | src/common.rs:528 |
| 65 | POST | `/api/ai/oa-fetch-refs` | `routes::api_ai_oa_fetch_refs` | src/common.rs:529 |
| 66 | POST | `/api/ai/oa-panel` | `routes::api_ai_oa_panel` | src/common.rs:530 |
| 67 | POST | `/api/ai/oa-history-compare` | `routes::api_ai_oa_history_compare` | src/common.rs:531 |
| 68 | POST | `/api/ai/oa-examiner-preview` | `routes::api_ai_oa_examiner_preview` | src/common.rs:535 |
| 69 | POST | `/api/ai/oa-defense-analysis` | `routes::api_ai_oa_defense_analysis` | src/common.rs:539 |
| 70 | POST | `/api/ai/oa-strategy-recommend` | `routes::api_ai_oa_strategy_recommend` | src/common.rs:543 |
| 71 | POST | `/api/ai/oa-quality-score` | `routes::api_ai_oa_quality_score` | src/common.rs:547 |
| 72 | POST | `/api/ai/audit-checks` | `routes::api_ai_audit_checks` | src/common.rs:551 |
| 73 | POST | `/api/ai/prohibition-check` | `routes::api_ai_prohibition_check` | src/common.rs:552 |
| 74 | POST | `/api/ai/oa-claim-simulate` | `routes::api_ai_oa_claim_simulate` | src/common.rs:556 |
| 75 | POST | `/api/ai/oa-collect-evidence` | `routes::api_ai_oa_collect_evidence` | src/common.rs:560 |
| 76 | POST | `/api/ai/oa-adversarial` | `routes::api_ai_oa_adversarial` | src/common.rs:564 |
| 77 | GET | `/api/oa/history/all` | `routes::api_oa_history_all` | src/common.rs:569 |
| 78 | GET | `/api/oa/history/:patent_number` | `routes::api_oa_history` | src/common.rs:570 |
| 79 | GET | `/api/oa/history/detail/:id` | `routes::api_oa_history_detail` | src/common.rs:574 |
| 80 | POST | `/api/oa/history/:id/delete` | `routes::api_oa_history_delete` | src/common.rs:578 |
| 81 | GET | `/api/oa/history/:patent_number/diff` | `routes::api_oa_history_diff` | src/common.rs:583 |
| 82 | GET | `/api/oa/discussions/:patent_number` | `routes::api_oa_discussion_list` | src/common.rs:588 |
| 83 | GET | `/api/oa/discussions/:patent_number/:discussion_id` | `routes::api_oa_discussion_get` | src/common.rs:592 |
| 84 | POST | `/api/oa/discussions/import` | `routes::api_oa_discussion_import` | src/common.rs:596 |
| 85 | POST | `/api/oa/export-docx` | `routes::api_oa_export_docx` | src/common.rs:601 |
| 86 | POST | `/api/oa/export` | `routes::oa_export::api_oa_export_post` | src/common.rs:603 |
| 87 | GET | `/api/oa/export/:filename` | `routes::oa_export::api_oa_export_get` | src/common.rs:607 |
| 88 | GET | `/api/oa/rounds/:patent_id` | `routes::api_oa_rounds_get` | src/common.rs:612 |
| 89 | POST | `/api/oa/rounds` | `routes::api_oa_rounds_create` | src/common.rs:613 |
| 90 | PUT | `/api/oa/rounds/update/:id` | `routes::api_oa_rounds_update` | src/common.rs:614 |
| 91 | GET | `/api/oa/timeline/:patent_id` | `routes::api_oa_timeline` | src/common.rs:618 |
| 92 | POST | `/api/ai/check-amendments` | `routes::api_ai_check_amendments` | src/common.rs:619 |
| 93 | POST | `/api/ai/threat-assessment` | `routes::api_ai_threat_assessment` | src/common.rs:624 |
| 94 | POST | `/api/ai/claim-chart` | `routes::api_ai_claim_chart` | src/common.rs:628 |
| 95 | GET | `/api/ai/cost` | `routes::api_ai_cost_summary` | src/common.rs:629 |
| 96 | GET | `/api/ai/cost/records` | `routes::api_ai_cost_records` | src/common.rs:630 |
| 97 | POST | `/api/ai/cost/record` | `routes::api_ai_cost_save` | src/common.rs:631 |
| 98 | POST | `/api/idea/submit` | `routes::api_idea_submit` | src/common.rs:633 |
| 99 | POST | `/api/idea/analyze` | `routes::api_idea_analyze` | src/common.rs:634 |
| 100 | POST | `/api/idea/pipeline` | `routes::api_idea_pipeline` | src/common.rs:635 |
| 101 | POST | `/api/ideas/batch-compare` | `routes::api_ideas_batch_compare` | src/common.rs:636 |
| 102 | GET | `/api/idea/list` | `routes::api_idea_list` | src/common.rs:640 |
| 103 | GET | `/api/idea/:id` | `routes::api_idea_get` | src/common.rs:641 |
| 104 | POST | `/api/idea/:id/delete` | `routes::api_idea_delete` | src/common.rs:642 |
| 105 | GET | `/api/idea/:id/progress` | `routes::api_idea_progress` | src/common.rs:643 |
| 106 | POST | `/api/idea/:id/resume` | `routes::api_idea_resume` | src/common.rs:644 |
| 107 | GET / POST | `/api/idea/:id/research-state` | `routes::api_idea_research_state` | src/common.rs:645 |
| 108 | POST | `/api/idea/:id/redirect` | `routes::api_idea_redirect` | src/common.rs:649 |
| 109 | GET | `/api/idea/:id/report` | `routes::api_idea_report` | src/common.rs:650 |
| 110 | GET | `/api/idea/:id/report.html` | `routes::api_idea_report_html` | src/common.rs:651 |
| 111 | GET | `/api/ai/models` | `routes::list_ai_models` | src/common.rs:655 |
| 112 | POST | `/api/idea/:id/chat` | `routes::api_idea_chat` | src/common.rs:656 |
| 113 | GET | `/api/idea/:id/evidence` | `routes::api_idea_evidence` | src/common.rs:657 |
| 114 | GET | `/api/idea/:id/kernel-report` | `routes::api_idea_kernel_report` | src/common.rs:658 |
| 115 | GET | `/api/idea/:id/messages` | `routes::api_idea_messages` | src/common.rs:662 |
| 116 | GET | `/api/idea/:id/chat/conclusions` | `routes::api_idea_chat_conclusions` | src/common.rs:663 |
| 117 | GET | `/api/idea/:id/chat/conversation` | `routes::api_idea_chat_conversation` | src/common.rs:667 |
| 118 | POST | `/api/idea/:id/summarize` | `routes::api_idea_summarize_discussion` | src/common.rs:671 |
| 119 | GET / POST | `/api/ideas/:id/feature-cards` | `routes::api_get_feature_cards` | src/common.rs:676 |
| 120 | GET | `/api/feature-cards/diff` | `routes::api_feature_card_diff` | src/common.rs:680 |
| 121 | GET | `/api/idea/:id/claim-tree` | `routes::api_idea_claim_tree` | src/common.rs:685 |
| 122 | POST | `/api/idea/:id/iterate` | `routes::api_idea_iterate` | src/common.rs:686 |
| 123 | GET | `/api/idea/:id/versions` | `routes::api_idea_versions` | src/common.rs:687 |
| 124 | GET | `/api/idea/:id/branches` | `routes::api_idea_branches` | src/common.rs:688 |
| 125 | GET | `/api/idea/:id/findings` | `routes::api_idea_findings` | src/common.rs:689 |
| 126 | GET / POST | `/api/idea/:id/memory` | `routes::api_idea_memory` | src/common.rs:691 |
| 127 | DELETE | `/api/idea/:id/memory/:entry_id` | `routes::api_idea_memory_delete` | src/common.rs:695 |
| 128 | GET | `/api/ipc/tree` | `routes::api_ipc_tree` | src/common.rs:700 |
| 129 | GET | `/api/ipc/:code/patents` | `routes::api_ipc_patents` | src/common.rs:701 |
| 130 | POST | `/api/patents/import` | `routes::api_import_patents` | src/common.rs:703 |
| 131 | GET / POST | `/api/collections` | `routes::api_list_collections` | src/common.rs:705 |
| 132 | DELETE | `/api/collections/:id` | `routes::api_delete_collection` | src/common.rs:709 |
| 133 | GET | `/api/collections/:id/patents` | `routes::api_get_collection_patents` | src/common.rs:713 |
| 134 | POST | `/api/collections/:id/add` | `routes::api_add_to_collection` | src/common.rs:717 |
| 135 | DELETE | `/api/collections/:id/remove/:patent_id` | `routes::api_remove_from_collection` | src/common.rs:721 |
| 136 | GET / POST | `/api/patents/:id/tags` | `routes::api_get_patent_tags` | src/common.rs:726 |
| 137 | DELETE | `/api/patents/:id/tags/:tag` | `routes::api_remove_tag` | src/common.rs:730 |
| 138 | GET | `/api/patents/:id/collections` | `routes::api_get_patent_collections` | src/common.rs:734 |
| 139 | GET | `/api/tags` | `routes::api_list_all_tags` | src/common.rs:738 |
| 140 | POST | `/api/upload/compare` | `routes::api_upload_compare` | src/common.rs:740 |
| 141 | POST | `/api/upload/extract` | `routes::api_upload_extract` | src/common.rs:741 |
| 142 | POST | `/api/upload/pdf-store` | `routes::api_upload_pdf_store` | src/common.rs:742 |
| 143 | GET | `/static/*path` | `serve_static_embedded` | src/common.rs:746 |
