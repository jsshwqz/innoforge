# 路由清单（自动生成）

> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。
> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：
> `node scripts/gen_route_inventory.js`
> 逐条「收什么、回什么」的契约（路径参数 / 请求体字段 / 响应键）见 `docs/API-CONTRACTS.md`，同样自动生成。

共 **165** 条路由：页面/静态 9 条，`/api` 接口 156 条。

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
| 23 | GET / POST | `/api/settings/ai-budget` | `routes::api_get_ai_budget` | src/common.rs:482 |
| 24 | GET | `/api/settings/serpapi/balance` | `routes::api_serpapi_balance` | src/common.rs:486 |
| 25 | GET | `/api/chat/:session_key` | `routes::api_chat_get_messages` | src/common.rs:491 |
| 26 | POST | `/api/chat/:session_key/save` | `routes::api_chat_save_message` | src/common.rs:492 |
| 27 | POST | `/api/chat/:session_key/delete` | `routes::api_chat_delete_messages` | src/common.rs:496 |
| 28 | POST | `/api/search` | `routes::api_search` | src/common.rs:501 |
| 29 | POST | `/api/search/stats` | `routes::api_search_stats` | src/common.rs:502 |
| 30 | POST | `/api/search/export` | `routes::api_export_csv` | src/common.rs:503 |
| 31 | POST | `/api/search/export/xlsx` | `routes::api_export_xlsx` | src/common.rs:504 |
| 32 | POST | `/api/search/online` | `routes::api_search_online` | src/common.rs:505 |
| 33 | POST | `/api/search/vector` | `routes::api_search_vector` | src/common.rs:506 |
| 34 | POST | `/api/search/analyze` | `routes::api_ai_analyze_results` | src/common.rs:507 |
| 35 | POST | `/api/patent/fetch` | `routes::api_fetch_patent` | src/common.rs:509 |
| 36 | GET | `/api/patent/enrich/:id` | `routes::api_enrich_patent` | src/common.rs:510 |
| 37 | GET | `/api/patent/enrich-free/:id` | `routes::api_enrich_patent_free` | src/common.rs:511 |
| 38 | POST | `/api/patent/:id/extract-pdf-drawings` | `routes::api_extract_patent_pdf_drawings` | src/common.rs:515 |
| 39 | GET | `/api/patents/backfill-status` | `routes::api_patents_backfill_status` | src/common.rs:519 |
| 40 | POST | `/api/patents/backfill-index` | `routes::api_patents_backfill_index` | src/common.rs:523 |
| 41 | GET | `/api/patent/pdf/:id` | `routes::api_patent_pdf` | src/common.rs:527 |
| 42 | POST | `/api/patent/pdf/extract-text` | `routes::api_patent_pdf_extract_text` | src/common.rs:528 |
| 43 | GET | `/api/patent/image-proxy` | `routes::api_patent_image_proxy` | src/common.rs:532 |
| 44 | GET | `/api/patent/lookup/:number` | `routes::api_patent_lookup` | src/common.rs:536 |
| 45 | POST | `/api/patent/lookup-or-fetch` | `routes::api_patent_lookup_and_fetch` | src/common.rs:537 |
| 46 | POST | `/api/patent/resolve-chinese-titles` | `routes::api_resolve_chinese_titles` | src/common.rs:541 |
| 47 | GET | `/api/patent/similar/:id` | `routes::api_recommend_similar` | src/common.rs:545 |
| 48 | GET / POST / PATCH | `/api/patent/:id/legal-status` | `routes::api_patent_legal_status` | src/common.rs:549 |
| 49 | GET | `/api/patent/:id/citation-graph` | `routes::api_patent_citation_graph` | src/common.rs:555 |
| 50 | POST | `/api/ai/chat` | `routes::api_ai_chat` | src/common.rs:560 |
| 51 | POST | `/api/ai/chat/stream` | `routes::api_ai_chat_stream` | src/common.rs:561 |
| 52 | POST | `/api/ai/chat/conclusions` | `routes::api_ai_chat_conclusions` | src/common.rs:562 |
| 53 | GET | `/api/auth/google/url` | `routes::api_google_auth_url` | src/common.rs:567 |
| 54 | GET | `/api/auth/google/callback` | `routes::api_google_callback` | src/common.rs:568 |
| 55 | POST | `/api/auth/google/exchange` | `routes::api_google_exchange_handler` | src/common.rs:572 |
| 56 | GET | `/api/auth/google/status` | `routes::api_google_oauth_status` | src/common.rs:576 |
| 57 | GET | `/api/auth/gcloud/status` | `routes::api_gcloud_status` | src/common.rs:581 |
| 58 | POST | `/api/auth/gcloud/login` | `routes::api_gcloud_login` | src/common.rs:582 |
| 59 | POST | `/api/ai/summarize` | `routes::api_ai_summarize` | src/common.rs:583 |
| 60 | POST | `/api/ai/compare` | `routes::api_ai_compare` | src/common.rs:584 |
| 61 | POST | `/api/ai/claims` | `routes::api_ai_claims_analysis` | src/common.rs:585 |
| 62 | POST | `/api/ai/risk` | `routes::api_ai_risk_assessment` | src/common.rs:586 |
| 63 | POST | `/api/ai/compare-matrix` | `routes::api_ai_compare_matrix` | src/common.rs:587 |
| 64 | POST | `/api/ai/batch-summarize` | `routes::api_ai_batch_summarize` | src/common.rs:591 |
| 65 | POST | `/api/ai/inventiveness-analysis` | `routes::api_ai_inventiveness_analysis` | src/common.rs:595 |
| 66 | POST | `/api/ai/office-action-response` | `routes::api_ai_office_action_response` | src/common.rs:599 |
| 67 | POST | `/api/ai/office-action-response/stream` | `routes::api_ai_office_action_response_stream` | src/common.rs:603 |
| 68 | POST | `/api/ai/oa-generate-response-letter` | `routes::api_ai_oa_generate_response_letter` | src/common.rs:607 |
| 69 | POST | `/api/ai/oa-discuss` | `routes::api_ai_oa_discuss` | src/common.rs:611 |
| 70 | POST | `/api/ai/oa-fetch-refs` | `routes::api_ai_oa_fetch_refs` | src/common.rs:612 |
| 71 | POST | `/api/ai/oa-panel` | `routes::api_ai_oa_panel` | src/common.rs:613 |
| 72 | POST | `/api/ai/oa-history-compare` | `routes::api_ai_oa_history_compare` | src/common.rs:614 |
| 73 | POST | `/api/ai/oa-examiner-preview` | `routes::api_ai_oa_examiner_preview` | src/common.rs:618 |
| 74 | POST | `/api/ai/oa-defense-analysis` | `routes::api_ai_oa_defense_analysis` | src/common.rs:622 |
| 75 | POST | `/api/ai/oa-strategy-recommend` | `routes::api_ai_oa_strategy_recommend` | src/common.rs:626 |
| 76 | POST | `/api/ai/oa-quality-score` | `routes::api_ai_oa_quality_score` | src/common.rs:630 |
| 77 | POST | `/api/ai/audit-checks` | `routes::api_ai_audit_checks` | src/common.rs:634 |
| 78 | POST | `/api/ai/prohibition-check` | `routes::api_ai_prohibition_check` | src/common.rs:635 |
| 79 | POST | `/api/ai/oa-claim-simulate` | `routes::api_ai_oa_claim_simulate` | src/common.rs:639 |
| 80 | POST | `/api/ai/oa-collect-evidence` | `routes::api_ai_oa_collect_evidence` | src/common.rs:643 |
| 81 | POST | `/api/ai/oa-adversarial` | `routes::api_ai_oa_adversarial` | src/common.rs:647 |
| 82 | POST | `/api/ai/vision-figure-check` | `routes::api_ai_vision_figure_check` | src/common.rs:651 |
| 83 | POST | `/api/ai/oa-feature-matrix` | `routes::api_ai_oa_feature_matrix` | src/common.rs:655 |
| 84 | GET | `/api/oa/history/all` | `routes::api_oa_history_all` | src/common.rs:660 |
| 85 | GET | `/api/oa/history/:patent_number` | `routes::api_oa_history` | src/common.rs:661 |
| 86 | GET | `/api/oa/history/detail/:id` | `routes::api_oa_history_detail` | src/common.rs:665 |
| 87 | POST | `/api/oa/history/:id/delete` | `routes::api_oa_history_delete` | src/common.rs:669 |
| 88 | GET | `/api/oa/history/:patent_number/diff` | `routes::api_oa_history_diff` | src/common.rs:674 |
| 89 | GET | `/api/oa/discussions/:patent_number` | `routes::api_oa_discussion_list` | src/common.rs:679 |
| 90 | GET | `/api/oa/discussions/:patent_number/:discussion_id` | `routes::api_oa_discussion_get` | src/common.rs:683 |
| 91 | POST | `/api/oa/discussions/import` | `routes::api_oa_discussion_import` | src/common.rs:687 |
| 92 | POST | `/api/oa/export-docx` | `routes::api_oa_export_docx` | src/common.rs:692 |
| 93 | GET | `/api/oa/:case_id/export/docx` | `routes::api_oa_case_export_docx` | src/common.rs:693 |
| 94 | POST | `/api/oa/export` | `routes::oa_export::api_oa_export_post` | src/common.rs:698 |
| 95 | GET | `/api/oa/export/:filename` | `routes::oa_export::api_oa_export_get` | src/common.rs:702 |
| 96 | GET | `/api/oa/rounds/:patent_id` | `routes::api_oa_rounds_get` | src/common.rs:707 |
| 97 | GET | `/api/oa/:case_id/rounds/diff` | `routes::api_oa_rounds_diff` | src/common.rs:708 |
| 98 | POST | `/api/oa/rounds` | `routes::api_oa_rounds_create` | src/common.rs:712 |
| 99 | PUT | `/api/oa/rounds/update/:id` | `routes::api_oa_rounds_update` | src/common.rs:713 |
| 100 | GET | `/api/oa/timeline/:patent_id` | `routes::api_oa_timeline` | src/common.rs:717 |
| 101 | POST | `/api/ai/check-amendments` | `routes::api_ai_check_amendments` | src/common.rs:718 |
| 102 | POST | `/api/ai/threat-assessment` | `routes::api_ai_threat_assessment` | src/common.rs:723 |
| 103 | POST | `/api/ai/claim-chart` | `routes::api_ai_claim_chart` | src/common.rs:727 |
| 104 | GET | `/api/ai/cost` | `routes::api_ai_cost_summary` | src/common.rs:728 |
| 105 | GET | `/api/ai/cost/records` | `routes::api_ai_cost_records` | src/common.rs:729 |
| 106 | POST | `/api/ai/cost/record` | `routes::api_ai_cost_save` | src/common.rs:730 |
| 107 | POST | `/api/idea/submit` | `routes::api_idea_submit` | src/common.rs:732 |
| 108 | POST | `/api/idea/analyze` | `routes::api_idea_analyze` | src/common.rs:733 |
| 109 | POST | `/api/idea/pipeline` | `routes::api_idea_pipeline` | src/common.rs:734 |
| 110 | POST | `/api/ideas/batch-compare` | `routes::api_ideas_batch_compare` | src/common.rs:735 |
| 111 | GET | `/api/idea/list` | `routes::api_idea_list` | src/common.rs:739 |
| 112 | GET | `/api/idea/:id` | `routes::api_idea_get` | src/common.rs:740 |
| 113 | POST | `/api/idea/:id/delete` | `routes::api_idea_delete` | src/common.rs:741 |
| 114 | GET | `/api/idea/:id/progress` | `routes::api_idea_progress` | src/common.rs:742 |
| 115 | POST | `/api/idea/:id/resume` | `routes::api_idea_resume` | src/common.rs:743 |
| 116 | GET / POST | `/api/idea/:id/research-state` | `routes::api_idea_research_state` | src/common.rs:744 |
| 117 | POST | `/api/idea/:id/redirect` | `routes::api_idea_redirect` | src/common.rs:748 |
| 118 | GET | `/api/idea/:id/report` | `routes::api_idea_report` | src/common.rs:749 |
| 119 | GET | `/api/idea/:id/report.html` | `routes::api_idea_report_html` | src/common.rs:750 |
| 120 | GET | `/api/ai/models` | `routes::list_ai_models` | src/common.rs:754 |
| 121 | POST | `/api/idea/:id/chat` | `routes::api_idea_chat` | src/common.rs:755 |
| 122 | GET | `/api/idea/:id/evidence` | `routes::api_idea_evidence` | src/common.rs:756 |
| 123 | GET | `/api/idea/:id/kernel-report` | `routes::api_idea_kernel_report` | src/common.rs:757 |
| 124 | GET | `/api/idea/:id/messages` | `routes::api_idea_messages` | src/common.rs:761 |
| 125 | GET | `/api/idea/:id/token-usage` | `routes::api_idea_token_usage` | src/common.rs:762 |
| 126 | GET | `/api/idea/:id/chat/conclusions` | `routes::api_idea_chat_conclusions` | src/common.rs:766 |
| 127 | GET | `/api/idea/:id/chat/conversation` | `routes::api_idea_chat_conversation` | src/common.rs:770 |
| 128 | POST | `/api/idea/:id/summarize` | `routes::api_idea_summarize_discussion` | src/common.rs:774 |
| 129 | GET / POST | `/api/ideas/:id/feature-cards` | `routes::api_get_feature_cards` | src/common.rs:779 |
| 130 | POST | `/api/ideas/:id/feature-cards/generate` | `routes::api_generate_feature_cards` | src/common.rs:783 |
| 131 | GET | `/api/ideas/:id/delivery-pack/xlsx` | `routes::api_idea_delivery_pack_xlsx` | src/common.rs:787 |
| 132 | POST | `/api/ideas/:id/control-prototype` | `routes::api_idea_control_prototype` | src/common.rs:791 |
| 133 | POST | `/api/ideas/:id/fto-scan` | `routes::api_idea_fto_scan` | src/common.rs:795 |
| 134 | GET | `/api/ideas/:id/archive/zip` | `routes::api_idea_archive_zip` | src/common.rs:796 |
| 135 | GET | `/api/ideas/:id/paper/latex-bundle` | `routes::api_idea_paper_latex_bundle` | src/common.rs:800 |
| 136 | GET | `/api/ideas/:id/paper/docx` | `routes::api_idea_paper_docx` | src/common.rs:804 |
| 137 | GET | `/api/feature-cards/diff` | `routes::api_feature_card_diff` | src/common.rs:808 |
| 138 | PUT | `/api/ideas/:id/claims` | `routes::api_idea_claims_replace` | src/common.rs:812 |
| 139 | POST | `/api/ideas/:id/claims/validate` | `routes::api_idea_claims_validate` | src/common.rs:816 |
| 140 | GET / POST | `/api/idea/:id/claim-tree` | `routes::api_idea_claim_tree` | src/common.rs:821 |
| 141 | DELETE | `/api/idea/:id/claim-tree/:claim_id` | `routes::api_idea_claim_node_delete` | src/common.rs:825 |
| 142 | POST | `/api/idea/:id/iterate` | `routes::api_idea_iterate` | src/common.rs:829 |
| 143 | GET | `/api/idea/:id/versions` | `routes::api_idea_versions` | src/common.rs:830 |
| 144 | GET | `/api/idea/:id/branches` | `routes::api_idea_branches` | src/common.rs:831 |
| 145 | GET | `/api/idea/:id/findings` | `routes::api_idea_findings` | src/common.rs:832 |
| 146 | GET / POST | `/api/idea/:id/memory` | `routes::api_idea_memory` | src/common.rs:834 |
| 147 | DELETE | `/api/idea/:id/memory/:entry_id` | `routes::api_idea_memory_delete` | src/common.rs:838 |
| 148 | GET | `/api/ipc/tree` | `routes::api_ipc_tree` | src/common.rs:843 |
| 149 | GET | `/api/ipc/:code/patents` | `routes::api_ipc_patents` | src/common.rs:844 |
| 150 | POST | `/api/patents/import` | `routes::api_import_patents` | src/common.rs:846 |
| 151 | POST | `/api/backup/export` | `routes::api_backup_export` | src/common.rs:847 |
| 152 | POST | `/api/backup/check` | `routes::api_backup_check` | src/common.rs:848 |
| 153 | GET / POST | `/api/collections` | `routes::api_list_collections` | src/common.rs:850 |
| 154 | DELETE | `/api/collections/:id` | `routes::api_delete_collection` | src/common.rs:854 |
| 155 | GET | `/api/collections/:id/patents` | `routes::api_get_collection_patents` | src/common.rs:858 |
| 156 | POST | `/api/collections/:id/add` | `routes::api_add_to_collection` | src/common.rs:862 |
| 157 | DELETE | `/api/collections/:id/remove/:patent_id` | `routes::api_remove_from_collection` | src/common.rs:866 |
| 158 | GET / POST | `/api/patents/:id/tags` | `routes::api_get_patent_tags` | src/common.rs:871 |
| 159 | DELETE | `/api/patents/:id/tags/:tag` | `routes::api_remove_tag` | src/common.rs:875 |
| 160 | GET | `/api/patents/:id/collections` | `routes::api_get_patent_collections` | src/common.rs:879 |
| 161 | GET | `/api/tags` | `routes::api_list_all_tags` | src/common.rs:883 |
| 162 | POST | `/api/upload/compare` | `routes::api_upload_compare` | src/common.rs:885 |
| 163 | POST | `/api/upload/extract` | `routes::api_upload_extract` | src/common.rs:886 |
| 164 | POST | `/api/upload/pdf-store` | `routes::api_upload_pdf_store` | src/common.rs:887 |
| 165 | GET | `/static/*path` | `serve_static_embedded` | src/common.rs:891 |
