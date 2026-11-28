# 路由清单（自动生成）

> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。
> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：
> `node scripts/gen_route_inventory.js`
> 逐条「收什么、回什么」的契约（路径参数 / 请求体字段 / 响应键）见 `docs/API-CONTRACTS.md`，同样自动生成。

共 **185** 条路由：页面/静态 9 条，`/api` 接口 176 条。

## 说明：清单只回答「有哪些端点」

清单保证「有哪些端点、处理函数是哪个、在 common.rs 第几行」；字段级契约在 `docs/API-CONTRACTS.md`；
叙述性说明、示例与业务语义在 `docs/API.md`。三份文件分工不同，任何一份都不假装自己覆盖了另外两份。

| # | 方法 | 路径 | 处理函数 | 源码位置 |
| --- | --- | --- | --- | --- |
| 1 | GET | `/` | `routes::index_page` | src/common.rs:521 |
| 2 | GET | `/search` | `routes::search_page` | src/common.rs:522 |
| 3 | GET | `/patent/:id` | `routes::patent_detail_page` | src/common.rs:523 |
| 4 | GET | `/ai` | `routes::ai_page` | src/common.rs:524 |
| 5 | GET | `/compare` | `routes::compare_page` | src/common.rs:525 |
| 6 | GET | `/idea` | `routes::idea_page` | src/common.rs:526 |
| 7 | GET | `/settings` | `routes::settings_page` | src/common.rs:527 |
| 8 | GET | `/oa-response` | `routes::office_action_response_page` | src/common.rs:528 |
| 9 | GET | `/api/cad/status` | `routes::api_cad_status` | src/common.rs:530 |
| 10 | POST | `/api/cad/start` | `routes::api_cad_start` | src/common.rs:531 |
| 11 | POST | `/api/cad/draw` | `routes::api_cad_draw` | src/common.rs:532 |
| 12 | GET | `/api/cad/artifacts` | `routes::api_cad_artifacts` | src/common.rs:533 |
| 13 | GET | `/api/cad/artifacts/:id/preview` | `routes::api_cad_preview` | src/common.rs:534 |
| 14 | GET | `/api/cad/artifacts/:id/download/:format` | `routes::api_cad_download` | src/common.rs:538 |
| 15 | GET | `/api/version` | `routes::api_get_version` | src/common.rs:543 |
| 16 | GET | `/api/oa/deadline-table` | `crate::oa_deadline::deadline_table` | src/common.rs:545 |
| 17 | GET | `/api/oa/compute-deadline` | `crate::oa_deadline::compute_deadline_api` | src/common.rs:549 |
| 18 | GET | `/api/settings` | `routes::api_get_settings` | src/common.rs:553 |
| 19 | POST | `/api/settings/cad` | `routes::api_save_cad_settings` | src/common.rs:554 |
| 20 | POST | `/api/settings/search-agent` | `routes::api_save_search_agent_settings` | src/common.rs:555 |
| 21 | POST | `/api/settings/serpapi` | `routes::api_save_serpapi` | src/common.rs:559 |
| 22 | POST | `/api/settings/ai` | `routes::api_save_ai` | src/common.rs:560 |
| 23 | GET / POST | `/api/settings/ai-budget` | `routes::api_get_ai_budget` | src/common.rs:561 |
| 24 | GET | `/api/settings/model-router` | `routes::api_get_model_router` | src/common.rs:565 |
| 25 | GET | `/api/settings/serpapi/balance` | `routes::api_serpapi_balance` | src/common.rs:569 |
| 26 | POST | `/api/settings/epo/test` | `routes::api_test_epo_credentials` | src/common.rs:573 |
| 27 | GET | `/api/chat/:session_key` | `routes::api_chat_get_messages` | src/common.rs:578 |
| 28 | POST | `/api/chat/:session_key/save` | `routes::api_chat_save_message` | src/common.rs:579 |
| 29 | POST | `/api/chat/:session_key/delete` | `routes::api_chat_delete_messages` | src/common.rs:583 |
| 30 | POST | `/api/search` | `routes::api_search` | src/common.rs:588 |
| 31 | POST | `/api/search/stats` | `routes::api_search_stats` | src/common.rs:589 |
| 32 | POST | `/api/search/export` | `routes::api_export_csv` | src/common.rs:590 |
| 33 | POST | `/api/search/export/xlsx` | `routes::api_export_xlsx` | src/common.rs:591 |
| 34 | POST | `/api/search/online` | `routes::api_search_online` | src/common.rs:592 |
| 35 | POST | `/api/search/source` | `routes::api_search_single_source` | src/common.rs:593 |
| 36 | POST | `/api/search/omni` | `routes::api_search_omni` | src/common.rs:594 |
| 37 | POST | `/api/search/vector` | `routes::api_search_vector` | src/common.rs:595 |
| 38 | POST | `/api/search/expand-citations` | `routes::api_search_expand_citations` | src/common.rs:596 |
| 39 | POST | `/api/search/analyze` | `routes::api_ai_analyze_results` | src/common.rs:600 |
| 40 | POST | `/api/patent/fetch` | `routes::api_fetch_patent` | src/common.rs:602 |
| 41 | GET | `/api/patent/enrich/:id` | `routes::api_enrich_patent` | src/common.rs:603 |
| 42 | GET | `/api/patent/enrich-free/:id` | `routes::api_enrich_patent_free` | src/common.rs:604 |
| 43 | POST | `/api/patent/:id/extract-pdf-drawings` | `routes::api_extract_patent_pdf_drawings` | src/common.rs:608 |
| 44 | GET | `/api/patents/backfill-status` | `routes::api_patents_backfill_status` | src/common.rs:612 |
| 45 | POST | `/api/patents/backfill-index` | `routes::api_patents_backfill_index` | src/common.rs:616 |
| 46 | GET | `/api/patent/pdf/:id` | `routes::api_patent_pdf` | src/common.rs:620 |
| 47 | POST | `/api/patent/pdf/extract-text` | `routes::api_patent_pdf_extract_text` | src/common.rs:621 |
| 48 | GET | `/api/patent/image-proxy` | `routes::api_patent_image_proxy` | src/common.rs:625 |
| 49 | GET | `/api/patent/lookup/:number` | `routes::api_patent_lookup` | src/common.rs:629 |
| 50 | POST | `/api/patent/lookup-or-fetch` | `routes::api_patent_lookup_and_fetch` | src/common.rs:630 |
| 51 | POST | `/api/patent/resolve-chinese-titles` | `routes::api_resolve_chinese_titles` | src/common.rs:634 |
| 52 | GET | `/api/patent/similar/:id` | `routes::api_recommend_similar` | src/common.rs:638 |
| 53 | GET / POST / PATCH | `/api/patent/:id/legal-status` | `routes::api_patent_legal_status` | src/common.rs:642 |
| 54 | GET | `/api/patent/:id/citation-graph` | `routes::api_patent_citation_graph` | src/common.rs:648 |
| 55 | POST | `/api/ai/chat` | `routes::api_ai_chat` | src/common.rs:653 |
| 56 | POST | `/api/ai/chat/stream` | `routes::api_ai_chat_stream` | src/common.rs:654 |
| 57 | POST | `/api/ai/chat/conclusions` | `routes::api_ai_chat_conclusions` | src/common.rs:655 |
| 58 | GET | `/api/auth/google/url` | `routes::api_google_auth_url` | src/common.rs:660 |
| 59 | GET | `/api/auth/google/callback` | `routes::api_google_callback` | src/common.rs:661 |
| 60 | POST | `/api/auth/google/exchange` | `routes::api_google_exchange_handler` | src/common.rs:665 |
| 61 | GET | `/api/auth/google/status` | `routes::api_google_oauth_status` | src/common.rs:669 |
| 62 | GET | `/api/auth/gcloud/status` | `routes::api_gcloud_status` | src/common.rs:674 |
| 63 | POST | `/api/auth/gcloud/login` | `routes::api_gcloud_login` | src/common.rs:675 |
| 64 | POST | `/api/ai/summarize` | `routes::api_ai_summarize` | src/common.rs:676 |
| 65 | POST | `/api/ai/compare` | `routes::api_ai_compare` | src/common.rs:677 |
| 66 | POST | `/api/ai/claims` | `routes::api_ai_claims_analysis` | src/common.rs:678 |
| 67 | POST | `/api/ai/risk` | `routes::api_ai_risk_assessment` | src/common.rs:679 |
| 68 | POST | `/api/ai/compare-matrix` | `routes::api_ai_compare_matrix` | src/common.rs:680 |
| 69 | POST | `/api/ai/batch-summarize` | `routes::api_ai_batch_summarize` | src/common.rs:684 |
| 70 | POST | `/api/ai/inventiveness-analysis` | `routes::api_ai_inventiveness_analysis` | src/common.rs:688 |
| 71 | POST | `/api/ai/office-action-response` | `routes::api_ai_office_action_response` | src/common.rs:692 |
| 72 | POST | `/api/ai/office-action-response/stream` | `routes::api_ai_office_action_response_stream` | src/common.rs:696 |
| 73 | POST | `/api/ai/oa-generate-response-letter` | `routes::api_ai_oa_generate_response_letter` | src/common.rs:700 |
| 74 | POST | `/api/ai/oa-discuss` | `routes::api_ai_oa_discuss` | src/common.rs:704 |
| 75 | POST | `/api/ai/oa-fetch-refs` | `routes::api_ai_oa_fetch_refs` | src/common.rs:705 |
| 76 | POST | `/api/ai/oa-panel` | `routes::api_ai_oa_panel` | src/common.rs:706 |
| 77 | POST | `/api/ai/oa-history-compare` | `routes::api_ai_oa_history_compare` | src/common.rs:707 |
| 78 | POST | `/api/ai/oa-examiner-preview` | `routes::api_ai_oa_examiner_preview` | src/common.rs:711 |
| 79 | POST | `/api/ai/oa-defense-analysis` | `routes::api_ai_oa_defense_analysis` | src/common.rs:715 |
| 80 | POST | `/api/ai/oa-strategy-recommend` | `routes::api_ai_oa_strategy_recommend` | src/common.rs:719 |
| 81 | POST | `/api/ai/oa-quality-score` | `routes::api_ai_oa_quality_score` | src/common.rs:723 |
| 82 | POST | `/api/ai/audit-checks` | `routes::api_ai_audit_checks` | src/common.rs:727 |
| 83 | POST | `/api/ai/prohibition-check` | `routes::api_ai_prohibition_check` | src/common.rs:728 |
| 84 | POST | `/api/ai/oa-claim-simulate` | `routes::api_ai_oa_claim_simulate` | src/common.rs:732 |
| 85 | POST | `/api/ai/oa-collect-evidence` | `routes::api_ai_oa_collect_evidence` | src/common.rs:736 |
| 86 | POST | `/api/ai/oa-adversarial` | `routes::api_ai_oa_adversarial` | src/common.rs:740 |
| 87 | POST | `/api/ai/vision-figure-check` | `routes::api_ai_vision_figure_check` | src/common.rs:744 |
| 88 | POST | `/api/ai/oa-feature-matrix` | `routes::api_ai_oa_feature_matrix` | src/common.rs:748 |
| 89 | GET | `/api/oa/history/all` | `routes::api_oa_history_all` | src/common.rs:753 |
| 90 | GET | `/api/oa/history/:patent_number` | `routes::api_oa_history` | src/common.rs:754 |
| 91 | GET | `/api/oa/history/detail/:id` | `routes::api_oa_history_detail` | src/common.rs:758 |
| 92 | POST | `/api/oa/history/:id/delete` | `routes::api_oa_history_delete` | src/common.rs:762 |
| 93 | GET | `/api/oa/history/:patent_number/diff` | `routes::api_oa_history_diff` | src/common.rs:767 |
| 94 | GET | `/api/oa/discussions/:patent_number` | `routes::api_oa_discussion_list` | src/common.rs:772 |
| 95 | GET | `/api/oa/discussions/:patent_number/:discussion_id` | `routes::api_oa_discussion_get` | src/common.rs:776 |
| 96 | POST | `/api/oa/discussions/import` | `routes::api_oa_discussion_import` | src/common.rs:780 |
| 97 | POST | `/api/oa/export-docx` | `routes::api_oa_export_docx` | src/common.rs:785 |
| 98 | POST | `/api/export/docx` | `routes::api_export_markdown_docx` | src/common.rs:786 |
| 99 | GET | `/api/oa/:case_id/export/docx` | `routes::api_oa_case_export_docx` | src/common.rs:787 |
| 100 | POST | `/api/oa/export` | `routes::oa_export::api_oa_export_post` | src/common.rs:792 |
| 101 | GET | `/api/oa/export/:filename` | `routes::oa_export::api_oa_export_get` | src/common.rs:796 |
| 102 | GET | `/api/oa/rounds/:patent_id` | `routes::api_oa_rounds_get` | src/common.rs:801 |
| 103 | GET | `/api/oa/:case_id/rounds/diff` | `routes::api_oa_rounds_diff` | src/common.rs:802 |
| 104 | POST | `/api/oa/rounds` | `routes::api_oa_rounds_create` | src/common.rs:806 |
| 105 | PUT | `/api/oa/rounds/update/:id` | `routes::api_oa_rounds_update` | src/common.rs:807 |
| 106 | GET | `/api/oa/timeline/:patent_id` | `routes::api_oa_timeline` | src/common.rs:811 |
| 107 | POST | `/api/ai/check-amendments` | `routes::api_ai_check_amendments` | src/common.rs:812 |
| 108 | POST | `/api/ai/threat-assessment` | `routes::api_ai_threat_assessment` | src/common.rs:817 |
| 109 | POST | `/api/ai/claim-chart` | `routes::api_ai_claim_chart` | src/common.rs:821 |
| 110 | GET | `/api/ai/cost` | `routes::api_ai_cost_summary` | src/common.rs:822 |
| 111 | GET | `/api/ai/cost/records` | `routes::api_ai_cost_records` | src/common.rs:823 |
| 112 | POST | `/api/ai/cost/record` | `routes::api_ai_cost_save` | src/common.rs:824 |
| 113 | POST | `/api/idea/submit` | `routes::api_idea_submit` | src/common.rs:826 |
| 114 | POST | `/api/idea/decompose` | `routes::api_idea_decompose` | src/common.rs:827 |
| 115 | POST | `/api/idea/analyze` | `routes::api_idea_analyze` | src/common.rs:828 |
| 116 | POST | `/api/idea/pipeline` | `routes::api_idea_pipeline` | src/common.rs:829 |
| 117 | POST | `/api/ideas/batch-compare` | `routes::api_ideas_batch_compare` | src/common.rs:830 |
| 118 | GET | `/api/idea/list` | `routes::api_idea_list` | src/common.rs:834 |
| 119 | GET | `/api/idea/:id` | `routes::api_idea_get` | src/common.rs:835 |
| 120 | POST | `/api/idea/:id/delete` | `routes::api_idea_delete` | src/common.rs:836 |
| 121 | GET | `/api/idea/:id/progress` | `routes::api_idea_progress` | src/common.rs:837 |
| 122 | POST | `/api/idea/:id/resume` | `routes::api_idea_resume` | src/common.rs:838 |
| 123 | GET / POST | `/api/idea/:id/research-state` | `routes::api_idea_research_state` | src/common.rs:839 |
| 124 | POST | `/api/idea/:id/redirect` | `routes::api_idea_redirect` | src/common.rs:843 |
| 125 | GET | `/api/idea/:id/report` | `routes::api_idea_report` | src/common.rs:844 |
| 126 | GET | `/api/idea/:id/report.html` | `routes::api_idea_report_html` | src/common.rs:845 |
| 127 | GET | `/api/ai/models` | `routes::list_ai_models` | src/common.rs:849 |
| 128 | POST | `/api/idea/:id/chat` | `routes::api_idea_chat` | src/common.rs:850 |
| 129 | GET | `/api/idea/:id/evidence` | `routes::api_idea_evidence` | src/common.rs:851 |
| 130 | GET | `/api/idea/:id/kernel-report` | `routes::api_idea_kernel_report` | src/common.rs:852 |
| 131 | GET | `/api/idea/:id/messages` | `routes::api_idea_messages` | src/common.rs:856 |
| 132 | GET | `/api/idea/:id/token-usage` | `routes::api_idea_token_usage` | src/common.rs:857 |
| 133 | GET | `/api/idea/:id/chat/conclusions` | `routes::api_idea_chat_conclusions` | src/common.rs:861 |
| 134 | GET | `/api/idea/:id/chat/conversation` | `routes::api_idea_chat_conversation` | src/common.rs:865 |
| 135 | POST | `/api/idea/:id/summarize` | `routes::api_idea_summarize_discussion` | src/common.rs:869 |
| 136 | GET / POST | `/api/ideas/:id/feature-cards` | `routes::api_get_feature_cards` | src/common.rs:874 |
| 137 | POST | `/api/ideas/:id/feature-cards/generate` | `routes::api_generate_feature_cards` | src/common.rs:878 |
| 138 | GET | `/api/ideas/:id/delivery-pack/xlsx` | `routes::api_idea_delivery_pack_xlsx` | src/common.rs:882 |
| 139 | POST | `/api/ideas/:id/control-prototype` | `routes::api_idea_control_prototype` | src/common.rs:886 |
| 140 | POST | `/api/ideas/:id/fto-scan` | `routes::api_idea_fto_scan` | src/common.rs:890 |
| 141 | GET | `/api/ideas/:id/archive/zip` | `routes::api_idea_archive_zip` | src/common.rs:891 |
| 142 | GET | `/api/ideas/:id/paper/latex-bundle` | `routes::api_idea_paper_latex_bundle` | src/common.rs:895 |
| 143 | GET | `/api/ideas/:id/paper/docx` | `routes::api_idea_paper_docx` | src/common.rs:899 |
| 144 | GET | `/api/feature-cards/diff` | `routes::api_feature_card_diff` | src/common.rs:903 |
| 145 | PUT | `/api/ideas/:id/claims` | `routes::api_idea_claims_replace` | src/common.rs:907 |
| 146 | POST | `/api/ideas/:id/claims/validate` | `routes::api_idea_claims_validate` | src/common.rs:911 |
| 147 | GET / POST | `/api/idea/:id/claim-tree` | `routes::api_idea_claim_tree` | src/common.rs:916 |
| 148 | DELETE | `/api/idea/:id/claim-tree/:claim_id` | `routes::api_idea_claim_node_delete` | src/common.rs:920 |
| 149 | POST | `/api/idea/:id/iterate` | `routes::api_idea_iterate` | src/common.rs:924 |
| 150 | GET | `/api/idea/:id/checkpoints` | `routes::api_idea_checkpoints` | src/common.rs:925 |
| 151 | POST | `/api/idea/:id/checkpoint/rollback` | `routes::api_idea_checkpoint_rollback` | src/common.rs:929 |
| 152 | GET | `/api/idea/:id/versions` | `routes::api_idea_versions` | src/common.rs:933 |
| 153 | GET | `/api/idea/:id/branches` | `routes::api_idea_branches` | src/common.rs:934 |
| 154 | GET | `/api/idea/:id/findings` | `routes::api_idea_findings` | src/common.rs:935 |
| 155 | GET / POST | `/api/idea/:id/memory` | `routes::api_idea_memory` | src/common.rs:937 |
| 156 | DELETE | `/api/idea/:id/memory/:entry_id` | `routes::api_idea_memory_delete` | src/common.rs:941 |
| 157 | GET | `/api/ipc/tree` | `routes::api_ipc_tree` | src/common.rs:946 |
| 158 | GET | `/api/ipc/:code/patents` | `routes::api_ipc_patents` | src/common.rs:947 |
| 159 | POST | `/api/patents/import` | `routes::api_import_patents` | src/common.rs:949 |
| 160 | POST | `/api/patents/import-table` | `routes::api_import_patent_table` | src/common.rs:950 |
| 161 | GET / POST | `/api/patents/sync-folder` | `routes::api_get_patent_sync_folder` | src/common.rs:954 |
| 162 | GET | `/api/patents/sync-folder/progress` | `routes::api_get_patent_sync_progress` | src/common.rs:958 |
| 163 | POST | `/api/patents/sync-folder/cancel` | `routes::api_cancel_patent_sync` | src/common.rs:962 |
| 164 | GET / POST | `/api/patents/remote-sync` | `routes::api_get_patent_remote_sync` | src/common.rs:966 |
| 165 | GET | `/api/patents/remote-sync/progress` | `routes::api_get_patent_remote_sync_progress` | src/common.rs:970 |
| 166 | POST | `/api/patents/remote-sync/cancel` | `routes::api_cancel_patent_remote_sync` | src/common.rs:974 |
| 167 | POST | `/api/backup/export` | `routes::api_backup_export` | src/common.rs:978 |
| 168 | POST | `/api/backup/check` | `routes::api_backup_check` | src/common.rs:979 |
| 169 | GET / POST | `/api/collections` | `routes::api_list_collections` | src/common.rs:981 |
| 170 | DELETE | `/api/collections/:id` | `routes::api_delete_collection` | src/common.rs:985 |
| 171 | GET | `/api/collections/:id/patents` | `routes::api_get_collection_patents` | src/common.rs:989 |
| 172 | POST | `/api/collections/:id/add` | `routes::api_add_to_collection` | src/common.rs:993 |
| 173 | DELETE | `/api/collections/:id/remove/:patent_id` | `routes::api_remove_from_collection` | src/common.rs:997 |
| 174 | GET / POST | `/api/patents/:id/tags` | `routes::api_get_patent_tags` | src/common.rs:1002 |
| 175 | DELETE | `/api/patents/:id/tags/:tag` | `routes::api_remove_tag` | src/common.rs:1006 |
| 176 | GET | `/api/patents/:id/collections` | `routes::api_get_patent_collections` | src/common.rs:1010 |
| 177 | GET | `/api/tags` | `routes::api_list_all_tags` | src/common.rs:1014 |
| 178 | POST | `/api/upload/compare` | `routes::api_upload_compare` | src/common.rs:1016 |
| 179 | POST | `/api/upload/ingest-local` | `routes::api_upload_ingest_local` | src/common.rs:1017 |
| 180 | POST | `/api/upload/extract` | `routes::api_upload_extract` | src/common.rs:1021 |
| 181 | POST | `/api/upload/extract/stream` | `routes::api_upload_extract_stream` | src/common.rs:1022 |
| 182 | POST | `/api/upload/pdf-store` | `routes::api_upload_pdf_store` | src/common.rs:1026 |
| 183 | GET | `/api/patent/:id/numeral-check` | `routes::api_patent_numeral_check` | src/common.rs:1028 |
| 184 | POST | `/api/ai/numeral-consistency-check` | `routes::api_ai_numeral_consistency_check` | src/common.rs:1032 |
| 185 | GET | `/static/*path` | `serve_static_embedded` | src/common.rs:1039 |
