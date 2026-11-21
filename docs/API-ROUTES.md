# 路由清单（自动生成）

> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。
> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：
> `node scripts/gen_route_inventory.js`
> 逐条「收什么、回什么」的契约（路径参数 / 请求体字段 / 响应键）见 `docs/API-CONTRACTS.md`，同样自动生成。

共 **161** 条路由：页面/静态 9 条，`/api` 接口 152 条。

## 说明：清单只回答「有哪些端点」

清单保证「有哪些端点、处理函数是哪个、在 common.rs 第几行」；字段级契约在 `docs/API-CONTRACTS.md`；
叙述性说明、示例与业务语义在 `docs/API.md`。三份文件分工不同，任何一份都不假装自己覆盖了另外两份。

| # | 方法 | 路径 | 处理函数 | 源码位置 |
| --- | --- | --- | --- | --- |
| 1 | GET | `/` | `routes::index_page` | src/common.rs:442 |
| 2 | GET | `/search` | `routes::search_page` | src/common.rs:443 |
| 3 | GET | `/patent/:id` | `routes::patent_detail_page` | src/common.rs:444 |
| 4 | GET | `/ai` | `routes::ai_page` | src/common.rs:445 |
| 5 | GET | `/compare` | `routes::compare_page` | src/common.rs:446 |
| 6 | GET | `/idea` | `routes::idea_page` | src/common.rs:447 |
| 7 | GET | `/settings` | `routes::settings_page` | src/common.rs:448 |
| 8 | GET | `/oa-response` | `routes::office_action_response_page` | src/common.rs:449 |
| 9 | GET | `/api/cad/status` | `routes::api_cad_status` | src/common.rs:451 |
| 10 | POST | `/api/cad/start` | `routes::api_cad_start` | src/common.rs:452 |
| 11 | POST | `/api/cad/draw` | `routes::api_cad_draw` | src/common.rs:453 |
| 12 | GET | `/api/cad/artifacts` | `routes::api_cad_artifacts` | src/common.rs:454 |
| 13 | GET | `/api/cad/artifacts/:id/preview` | `routes::api_cad_preview` | src/common.rs:455 |
| 14 | GET | `/api/cad/artifacts/:id/download/:format` | `routes::api_cad_download` | src/common.rs:459 |
| 15 | GET | `/api/version` | `routes::api_get_version` | src/common.rs:464 |
| 16 | GET | `/api/oa/deadline-table` | `crate::oa_deadline::deadline_table` | src/common.rs:466 |
| 17 | GET | `/api/oa/compute-deadline` | `crate::oa_deadline::compute_deadline_api` | src/common.rs:470 |
| 18 | GET | `/api/settings` | `routes::api_get_settings` | src/common.rs:474 |
| 19 | POST | `/api/settings/cad` | `routes::api_save_cad_settings` | src/common.rs:475 |
| 20 | POST | `/api/settings/search-agent` | `routes::api_save_search_agent_settings` | src/common.rs:476 |
| 21 | POST | `/api/settings/serpapi` | `routes::api_save_serpapi` | src/common.rs:480 |
| 22 | POST | `/api/settings/ai` | `routes::api_save_ai` | src/common.rs:481 |
| 23 | GET | `/api/settings/serpapi/balance` | `routes::api_serpapi_balance` | src/common.rs:482 |
| 24 | GET | `/api/chat/:session_key` | `routes::api_chat_get_messages` | src/common.rs:487 |
| 25 | POST | `/api/chat/:session_key/save` | `routes::api_chat_save_message` | src/common.rs:488 |
| 26 | POST | `/api/chat/:session_key/delete` | `routes::api_chat_delete_messages` | src/common.rs:492 |
| 27 | POST | `/api/search` | `routes::api_search` | src/common.rs:497 |
| 28 | POST | `/api/search/stats` | `routes::api_search_stats` | src/common.rs:498 |
| 29 | POST | `/api/search/export` | `routes::api_export_csv` | src/common.rs:499 |
| 30 | POST | `/api/search/export/xlsx` | `routes::api_export_xlsx` | src/common.rs:500 |
| 31 | POST | `/api/search/online` | `routes::api_search_online` | src/common.rs:501 |
| 32 | POST | `/api/search/vector` | `routes::api_search_vector` | src/common.rs:502 |
| 33 | POST | `/api/search/analyze` | `routes::api_ai_analyze_results` | src/common.rs:503 |
| 34 | POST | `/api/patent/fetch` | `routes::api_fetch_patent` | src/common.rs:505 |
| 35 | GET | `/api/patent/enrich/:id` | `routes::api_enrich_patent` | src/common.rs:506 |
| 36 | GET | `/api/patent/enrich-free/:id` | `routes::api_enrich_patent_free` | src/common.rs:507 |
| 37 | POST | `/api/patent/:id/extract-pdf-drawings` | `routes::api_extract_patent_pdf_drawings` | src/common.rs:511 |
| 38 | GET | `/api/patent/pdf/:id` | `routes::api_patent_pdf` | src/common.rs:515 |
| 39 | POST | `/api/patent/pdf/extract-text` | `routes::api_patent_pdf_extract_text` | src/common.rs:516 |
| 40 | GET | `/api/patent/image-proxy` | `routes::api_patent_image_proxy` | src/common.rs:520 |
| 41 | GET | `/api/patent/lookup/:number` | `routes::api_patent_lookup` | src/common.rs:524 |
| 42 | POST | `/api/patent/lookup-or-fetch` | `routes::api_patent_lookup_and_fetch` | src/common.rs:525 |
| 43 | POST | `/api/patent/resolve-chinese-titles` | `routes::api_resolve_chinese_titles` | src/common.rs:529 |
| 44 | GET | `/api/patent/similar/:id` | `routes::api_recommend_similar` | src/common.rs:533 |
| 45 | GET / POST / PATCH | `/api/patent/:id/legal-status` | `routes::api_patent_legal_status` | src/common.rs:537 |
| 46 | GET | `/api/patent/:id/citation-graph` | `routes::api_patent_citation_graph` | src/common.rs:543 |
| 47 | POST | `/api/ai/chat` | `routes::api_ai_chat` | src/common.rs:548 |
| 48 | POST | `/api/ai/chat/stream` | `routes::api_ai_chat_stream` | src/common.rs:549 |
| 49 | POST | `/api/ai/chat/conclusions` | `routes::api_ai_chat_conclusions` | src/common.rs:550 |
| 50 | GET | `/api/auth/google/url` | `routes::api_google_auth_url` | src/common.rs:555 |
| 51 | GET | `/api/auth/google/callback` | `routes::api_google_callback` | src/common.rs:556 |
| 52 | POST | `/api/auth/google/exchange` | `routes::api_google_exchange_handler` | src/common.rs:560 |
| 53 | GET | `/api/auth/google/status` | `routes::api_google_oauth_status` | src/common.rs:564 |
| 54 | GET | `/api/auth/gcloud/status` | `routes::api_gcloud_status` | src/common.rs:569 |
| 55 | POST | `/api/auth/gcloud/login` | `routes::api_gcloud_login` | src/common.rs:570 |
| 56 | POST | `/api/ai/summarize` | `routes::api_ai_summarize` | src/common.rs:571 |
| 57 | POST | `/api/ai/compare` | `routes::api_ai_compare` | src/common.rs:572 |
| 58 | POST | `/api/ai/claims` | `routes::api_ai_claims_analysis` | src/common.rs:573 |
| 59 | POST | `/api/ai/risk` | `routes::api_ai_risk_assessment` | src/common.rs:574 |
| 60 | POST | `/api/ai/compare-matrix` | `routes::api_ai_compare_matrix` | src/common.rs:575 |
| 61 | POST | `/api/ai/batch-summarize` | `routes::api_ai_batch_summarize` | src/common.rs:579 |
| 62 | POST | `/api/ai/inventiveness-analysis` | `routes::api_ai_inventiveness_analysis` | src/common.rs:583 |
| 63 | POST | `/api/ai/office-action-response` | `routes::api_ai_office_action_response` | src/common.rs:587 |
| 64 | POST | `/api/ai/office-action-response/stream` | `routes::api_ai_office_action_response_stream` | src/common.rs:591 |
| 65 | POST | `/api/ai/oa-generate-response-letter` | `routes::api_ai_oa_generate_response_letter` | src/common.rs:595 |
| 66 | POST | `/api/ai/oa-discuss` | `routes::api_ai_oa_discuss` | src/common.rs:599 |
| 67 | POST | `/api/ai/oa-fetch-refs` | `routes::api_ai_oa_fetch_refs` | src/common.rs:600 |
| 68 | POST | `/api/ai/oa-panel` | `routes::api_ai_oa_panel` | src/common.rs:601 |
| 69 | POST | `/api/ai/oa-history-compare` | `routes::api_ai_oa_history_compare` | src/common.rs:602 |
| 70 | POST | `/api/ai/oa-examiner-preview` | `routes::api_ai_oa_examiner_preview` | src/common.rs:606 |
| 71 | POST | `/api/ai/oa-defense-analysis` | `routes::api_ai_oa_defense_analysis` | src/common.rs:610 |
| 72 | POST | `/api/ai/oa-strategy-recommend` | `routes::api_ai_oa_strategy_recommend` | src/common.rs:614 |
| 73 | POST | `/api/ai/oa-quality-score` | `routes::api_ai_oa_quality_score` | src/common.rs:618 |
| 74 | POST | `/api/ai/audit-checks` | `routes::api_ai_audit_checks` | src/common.rs:622 |
| 75 | POST | `/api/ai/prohibition-check` | `routes::api_ai_prohibition_check` | src/common.rs:623 |
| 76 | POST | `/api/ai/oa-claim-simulate` | `routes::api_ai_oa_claim_simulate` | src/common.rs:627 |
| 77 | POST | `/api/ai/oa-collect-evidence` | `routes::api_ai_oa_collect_evidence` | src/common.rs:631 |
| 78 | POST | `/api/ai/oa-adversarial` | `routes::api_ai_oa_adversarial` | src/common.rs:635 |
| 79 | POST | `/api/ai/vision-figure-check` | `routes::api_ai_vision_figure_check` | src/common.rs:639 |
| 80 | POST | `/api/ai/oa-feature-matrix` | `routes::api_ai_oa_feature_matrix` | src/common.rs:643 |
| 81 | GET | `/api/oa/history/all` | `routes::api_oa_history_all` | src/common.rs:648 |
| 82 | GET | `/api/oa/history/:patent_number` | `routes::api_oa_history` | src/common.rs:649 |
| 83 | GET | `/api/oa/history/detail/:id` | `routes::api_oa_history_detail` | src/common.rs:653 |
| 84 | POST | `/api/oa/history/:id/delete` | `routes::api_oa_history_delete` | src/common.rs:657 |
| 85 | GET | `/api/oa/history/:patent_number/diff` | `routes::api_oa_history_diff` | src/common.rs:662 |
| 86 | GET | `/api/oa/discussions/:patent_number` | `routes::api_oa_discussion_list` | src/common.rs:667 |
| 87 | GET | `/api/oa/discussions/:patent_number/:discussion_id` | `routes::api_oa_discussion_get` | src/common.rs:671 |
| 88 | POST | `/api/oa/discussions/import` | `routes::api_oa_discussion_import` | src/common.rs:675 |
| 89 | POST | `/api/oa/export-docx` | `routes::api_oa_export_docx` | src/common.rs:680 |
| 90 | GET | `/api/oa/:case_id/export/docx` | `routes::api_oa_case_export_docx` | src/common.rs:681 |
| 91 | POST | `/api/oa/export` | `routes::oa_export::api_oa_export_post` | src/common.rs:686 |
| 92 | GET | `/api/oa/export/:filename` | `routes::oa_export::api_oa_export_get` | src/common.rs:690 |
| 93 | GET | `/api/oa/rounds/:patent_id` | `routes::api_oa_rounds_get` | src/common.rs:695 |
| 94 | GET | `/api/oa/:case_id/rounds/diff` | `routes::api_oa_rounds_diff` | src/common.rs:696 |
| 95 | POST | `/api/oa/rounds` | `routes::api_oa_rounds_create` | src/common.rs:700 |
| 96 | PUT | `/api/oa/rounds/update/:id` | `routes::api_oa_rounds_update` | src/common.rs:701 |
| 97 | GET | `/api/oa/timeline/:patent_id` | `routes::api_oa_timeline` | src/common.rs:705 |
| 98 | POST | `/api/ai/check-amendments` | `routes::api_ai_check_amendments` | src/common.rs:706 |
| 99 | POST | `/api/ai/threat-assessment` | `routes::api_ai_threat_assessment` | src/common.rs:711 |
| 100 | POST | `/api/ai/claim-chart` | `routes::api_ai_claim_chart` | src/common.rs:715 |
| 101 | GET | `/api/ai/cost` | `routes::api_ai_cost_summary` | src/common.rs:716 |
| 102 | GET | `/api/ai/cost/records` | `routes::api_ai_cost_records` | src/common.rs:717 |
| 103 | POST | `/api/ai/cost/record` | `routes::api_ai_cost_save` | src/common.rs:718 |
| 104 | POST | `/api/idea/submit` | `routes::api_idea_submit` | src/common.rs:720 |
| 105 | POST | `/api/idea/analyze` | `routes::api_idea_analyze` | src/common.rs:721 |
| 106 | POST | `/api/idea/pipeline` | `routes::api_idea_pipeline` | src/common.rs:722 |
| 107 | POST | `/api/ideas/batch-compare` | `routes::api_ideas_batch_compare` | src/common.rs:723 |
| 108 | GET | `/api/idea/list` | `routes::api_idea_list` | src/common.rs:727 |
| 109 | GET | `/api/idea/:id` | `routes::api_idea_get` | src/common.rs:728 |
| 110 | POST | `/api/idea/:id/delete` | `routes::api_idea_delete` | src/common.rs:729 |
| 111 | GET | `/api/idea/:id/progress` | `routes::api_idea_progress` | src/common.rs:730 |
| 112 | POST | `/api/idea/:id/resume` | `routes::api_idea_resume` | src/common.rs:731 |
| 113 | GET / POST | `/api/idea/:id/research-state` | `routes::api_idea_research_state` | src/common.rs:732 |
| 114 | POST | `/api/idea/:id/redirect` | `routes::api_idea_redirect` | src/common.rs:736 |
| 115 | GET | `/api/idea/:id/report` | `routes::api_idea_report` | src/common.rs:737 |
| 116 | GET | `/api/idea/:id/report.html` | `routes::api_idea_report_html` | src/common.rs:738 |
| 117 | GET | `/api/ai/models` | `routes::list_ai_models` | src/common.rs:742 |
| 118 | POST | `/api/idea/:id/chat` | `routes::api_idea_chat` | src/common.rs:743 |
| 119 | GET | `/api/idea/:id/evidence` | `routes::api_idea_evidence` | src/common.rs:744 |
| 120 | GET | `/api/idea/:id/kernel-report` | `routes::api_idea_kernel_report` | src/common.rs:745 |
| 121 | GET | `/api/idea/:id/messages` | `routes::api_idea_messages` | src/common.rs:749 |
| 122 | GET | `/api/idea/:id/chat/conclusions` | `routes::api_idea_chat_conclusions` | src/common.rs:750 |
| 123 | GET | `/api/idea/:id/chat/conversation` | `routes::api_idea_chat_conversation` | src/common.rs:754 |
| 124 | POST | `/api/idea/:id/summarize` | `routes::api_idea_summarize_discussion` | src/common.rs:758 |
| 125 | GET / POST | `/api/ideas/:id/feature-cards` | `routes::api_get_feature_cards` | src/common.rs:763 |
| 126 | POST | `/api/ideas/:id/feature-cards/generate` | `routes::api_generate_feature_cards` | src/common.rs:767 |
| 127 | GET | `/api/ideas/:id/delivery-pack/xlsx` | `routes::api_idea_delivery_pack_xlsx` | src/common.rs:771 |
| 128 | POST | `/api/ideas/:id/control-prototype` | `routes::api_idea_control_prototype` | src/common.rs:775 |
| 129 | POST | `/api/ideas/:id/fto-scan` | `routes::api_idea_fto_scan` | src/common.rs:779 |
| 130 | GET | `/api/ideas/:id/archive/zip` | `routes::api_idea_archive_zip` | src/common.rs:780 |
| 131 | GET | `/api/ideas/:id/paper/latex-bundle` | `routes::api_idea_paper_latex_bundle` | src/common.rs:784 |
| 132 | GET | `/api/ideas/:id/paper/docx` | `routes::api_idea_paper_docx` | src/common.rs:788 |
| 133 | GET | `/api/feature-cards/diff` | `routes::api_feature_card_diff` | src/common.rs:792 |
| 134 | PUT | `/api/ideas/:id/claims` | `routes::api_idea_claims_replace` | src/common.rs:796 |
| 135 | POST | `/api/ideas/:id/claims/validate` | `routes::api_idea_claims_validate` | src/common.rs:800 |
| 136 | GET / POST | `/api/idea/:id/claim-tree` | `routes::api_idea_claim_tree` | src/common.rs:805 |
| 137 | DELETE | `/api/idea/:id/claim-tree/:claim_id` | `routes::api_idea_claim_node_delete` | src/common.rs:809 |
| 138 | POST | `/api/idea/:id/iterate` | `routes::api_idea_iterate` | src/common.rs:813 |
| 139 | GET | `/api/idea/:id/versions` | `routes::api_idea_versions` | src/common.rs:814 |
| 140 | GET | `/api/idea/:id/branches` | `routes::api_idea_branches` | src/common.rs:815 |
| 141 | GET | `/api/idea/:id/findings` | `routes::api_idea_findings` | src/common.rs:816 |
| 142 | GET / POST | `/api/idea/:id/memory` | `routes::api_idea_memory` | src/common.rs:818 |
| 143 | DELETE | `/api/idea/:id/memory/:entry_id` | `routes::api_idea_memory_delete` | src/common.rs:822 |
| 144 | GET | `/api/ipc/tree` | `routes::api_ipc_tree` | src/common.rs:827 |
| 145 | GET | `/api/ipc/:code/patents` | `routes::api_ipc_patents` | src/common.rs:828 |
| 146 | POST | `/api/patents/import` | `routes::api_import_patents` | src/common.rs:830 |
| 147 | POST | `/api/backup/export` | `routes::api_backup_export` | src/common.rs:831 |
| 148 | POST | `/api/backup/check` | `routes::api_backup_check` | src/common.rs:832 |
| 149 | GET / POST | `/api/collections` | `routes::api_list_collections` | src/common.rs:834 |
| 150 | DELETE | `/api/collections/:id` | `routes::api_delete_collection` | src/common.rs:838 |
| 151 | GET | `/api/collections/:id/patents` | `routes::api_get_collection_patents` | src/common.rs:842 |
| 152 | POST | `/api/collections/:id/add` | `routes::api_add_to_collection` | src/common.rs:846 |
| 153 | DELETE | `/api/collections/:id/remove/:patent_id` | `routes::api_remove_from_collection` | src/common.rs:850 |
| 154 | GET / POST | `/api/patents/:id/tags` | `routes::api_get_patent_tags` | src/common.rs:855 |
| 155 | DELETE | `/api/patents/:id/tags/:tag` | `routes::api_remove_tag` | src/common.rs:859 |
| 156 | GET | `/api/patents/:id/collections` | `routes::api_get_patent_collections` | src/common.rs:863 |
| 157 | GET | `/api/tags` | `routes::api_list_all_tags` | src/common.rs:867 |
| 158 | POST | `/api/upload/compare` | `routes::api_upload_compare` | src/common.rs:869 |
| 159 | POST | `/api/upload/extract` | `routes::api_upload_extract` | src/common.rs:870 |
| 160 | POST | `/api/upload/pdf-store` | `routes::api_upload_pdf_store` | src/common.rs:871 |
| 161 | GET | `/static/*path` | `serve_static_embedded` | src/common.rs:875 |
