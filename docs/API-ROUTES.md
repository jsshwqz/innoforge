# 路由清单（自动生成）

> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。
> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：
> `node scripts/gen_route_inventory.js`
> 逐条「收什么、回什么」的契约（路径参数 / 请求体字段 / 响应键）见 `docs/API-CONTRACTS.md`，同样自动生成。

共 **181** 条路由：页面/静态 9 条，`/api` 接口 172 条。

## 说明：清单只回答「有哪些端点」

清单保证「有哪些端点、处理函数是哪个、在 common.rs 第几行」；字段级契约在 `docs/API-CONTRACTS.md`；
叙述性说明、示例与业务语义在 `docs/API.md`。三份文件分工不同，任何一份都不假装自己覆盖了另外两份。

| # | 方法 | 路径 | 处理函数 | 源码位置 |
| --- | --- | --- | --- | --- |
| 1 | GET | `/` | `routes::index_page` | src/common.rs:520 |
| 2 | GET | `/search` | `routes::search_page` | src/common.rs:521 |
| 3 | GET | `/patent/:id` | `routes::patent_detail_page` | src/common.rs:522 |
| 4 | GET | `/ai` | `routes::ai_page` | src/common.rs:523 |
| 5 | GET | `/compare` | `routes::compare_page` | src/common.rs:524 |
| 6 | GET | `/idea` | `routes::idea_page` | src/common.rs:525 |
| 7 | GET | `/settings` | `routes::settings_page` | src/common.rs:526 |
| 8 | GET | `/oa-response` | `routes::office_action_response_page` | src/common.rs:527 |
| 9 | GET | `/api/cad/status` | `routes::api_cad_status` | src/common.rs:529 |
| 10 | POST | `/api/cad/start` | `routes::api_cad_start` | src/common.rs:530 |
| 11 | POST | `/api/cad/draw` | `routes::api_cad_draw` | src/common.rs:531 |
| 12 | GET | `/api/cad/artifacts` | `routes::api_cad_artifacts` | src/common.rs:532 |
| 13 | GET | `/api/cad/artifacts/:id/preview` | `routes::api_cad_preview` | src/common.rs:533 |
| 14 | GET | `/api/cad/artifacts/:id/download/:format` | `routes::api_cad_download` | src/common.rs:537 |
| 15 | GET | `/api/version` | `routes::api_get_version` | src/common.rs:542 |
| 16 | GET | `/api/oa/deadline-table` | `crate::oa_deadline::deadline_table` | src/common.rs:544 |
| 17 | GET | `/api/oa/compute-deadline` | `crate::oa_deadline::compute_deadline_api` | src/common.rs:548 |
| 18 | GET | `/api/settings` | `routes::api_get_settings` | src/common.rs:552 |
| 19 | POST | `/api/settings/cad` | `routes::api_save_cad_settings` | src/common.rs:553 |
| 20 | POST | `/api/settings/search-agent` | `routes::api_save_search_agent_settings` | src/common.rs:554 |
| 21 | POST | `/api/settings/serpapi` | `routes::api_save_serpapi` | src/common.rs:558 |
| 22 | POST | `/api/settings/ai` | `routes::api_save_ai` | src/common.rs:559 |
| 23 | GET / POST | `/api/settings/ai-budget` | `routes::api_get_ai_budget` | src/common.rs:560 |
| 24 | GET | `/api/settings/model-router` | `routes::api_get_model_router` | src/common.rs:564 |
| 25 | GET | `/api/settings/serpapi/balance` | `routes::api_serpapi_balance` | src/common.rs:568 |
| 26 | POST | `/api/settings/epo/test` | `routes::api_test_epo_credentials` | src/common.rs:572 |
| 27 | GET | `/api/chat/:session_key` | `routes::api_chat_get_messages` | src/common.rs:577 |
| 28 | POST | `/api/chat/:session_key/save` | `routes::api_chat_save_message` | src/common.rs:578 |
| 29 | POST | `/api/chat/:session_key/delete` | `routes::api_chat_delete_messages` | src/common.rs:582 |
| 30 | POST | `/api/search` | `routes::api_search` | src/common.rs:587 |
| 31 | POST | `/api/search/stats` | `routes::api_search_stats` | src/common.rs:588 |
| 32 | POST | `/api/search/export` | `routes::api_export_csv` | src/common.rs:589 |
| 33 | POST | `/api/search/export/xlsx` | `routes::api_export_xlsx` | src/common.rs:590 |
| 34 | POST | `/api/search/online` | `routes::api_search_online` | src/common.rs:591 |
| 35 | POST | `/api/search/source` | `routes::api_search_single_source` | src/common.rs:592 |
| 36 | POST | `/api/search/omni` | `routes::api_search_omni` | src/common.rs:593 |
| 37 | POST | `/api/search/vector` | `routes::api_search_vector` | src/common.rs:594 |
| 38 | POST | `/api/search/analyze` | `routes::api_ai_analyze_results` | src/common.rs:595 |
| 39 | POST | `/api/patent/fetch` | `routes::api_fetch_patent` | src/common.rs:597 |
| 40 | GET | `/api/patent/enrich/:id` | `routes::api_enrich_patent` | src/common.rs:598 |
| 41 | GET | `/api/patent/enrich-free/:id` | `routes::api_enrich_patent_free` | src/common.rs:599 |
| 42 | POST | `/api/patent/:id/extract-pdf-drawings` | `routes::api_extract_patent_pdf_drawings` | src/common.rs:603 |
| 43 | GET | `/api/patents/backfill-status` | `routes::api_patents_backfill_status` | src/common.rs:607 |
| 44 | POST | `/api/patents/backfill-index` | `routes::api_patents_backfill_index` | src/common.rs:611 |
| 45 | GET | `/api/patent/pdf/:id` | `routes::api_patent_pdf` | src/common.rs:615 |
| 46 | POST | `/api/patent/pdf/extract-text` | `routes::api_patent_pdf_extract_text` | src/common.rs:616 |
| 47 | GET | `/api/patent/image-proxy` | `routes::api_patent_image_proxy` | src/common.rs:620 |
| 48 | GET | `/api/patent/lookup/:number` | `routes::api_patent_lookup` | src/common.rs:624 |
| 49 | POST | `/api/patent/lookup-or-fetch` | `routes::api_patent_lookup_and_fetch` | src/common.rs:625 |
| 50 | POST | `/api/patent/resolve-chinese-titles` | `routes::api_resolve_chinese_titles` | src/common.rs:629 |
| 51 | GET | `/api/patent/similar/:id` | `routes::api_recommend_similar` | src/common.rs:633 |
| 52 | GET / POST / PATCH | `/api/patent/:id/legal-status` | `routes::api_patent_legal_status` | src/common.rs:637 |
| 53 | GET | `/api/patent/:id/citation-graph` | `routes::api_patent_citation_graph` | src/common.rs:643 |
| 54 | POST | `/api/ai/chat` | `routes::api_ai_chat` | src/common.rs:648 |
| 55 | POST | `/api/ai/chat/stream` | `routes::api_ai_chat_stream` | src/common.rs:649 |
| 56 | POST | `/api/ai/chat/conclusions` | `routes::api_ai_chat_conclusions` | src/common.rs:650 |
| 57 | GET | `/api/auth/google/url` | `routes::api_google_auth_url` | src/common.rs:655 |
| 58 | GET | `/api/auth/google/callback` | `routes::api_google_callback` | src/common.rs:656 |
| 59 | POST | `/api/auth/google/exchange` | `routes::api_google_exchange_handler` | src/common.rs:660 |
| 60 | GET | `/api/auth/google/status` | `routes::api_google_oauth_status` | src/common.rs:664 |
| 61 | GET | `/api/auth/gcloud/status` | `routes::api_gcloud_status` | src/common.rs:669 |
| 62 | POST | `/api/auth/gcloud/login` | `routes::api_gcloud_login` | src/common.rs:670 |
| 63 | POST | `/api/ai/summarize` | `routes::api_ai_summarize` | src/common.rs:671 |
| 64 | POST | `/api/ai/compare` | `routes::api_ai_compare` | src/common.rs:672 |
| 65 | POST | `/api/ai/claims` | `routes::api_ai_claims_analysis` | src/common.rs:673 |
| 66 | POST | `/api/ai/risk` | `routes::api_ai_risk_assessment` | src/common.rs:674 |
| 67 | POST | `/api/ai/compare-matrix` | `routes::api_ai_compare_matrix` | src/common.rs:675 |
| 68 | POST | `/api/ai/batch-summarize` | `routes::api_ai_batch_summarize` | src/common.rs:679 |
| 69 | POST | `/api/ai/inventiveness-analysis` | `routes::api_ai_inventiveness_analysis` | src/common.rs:683 |
| 70 | POST | `/api/ai/office-action-response` | `routes::api_ai_office_action_response` | src/common.rs:687 |
| 71 | POST | `/api/ai/office-action-response/stream` | `routes::api_ai_office_action_response_stream` | src/common.rs:691 |
| 72 | POST | `/api/ai/oa-generate-response-letter` | `routes::api_ai_oa_generate_response_letter` | src/common.rs:695 |
| 73 | POST | `/api/ai/oa-discuss` | `routes::api_ai_oa_discuss` | src/common.rs:699 |
| 74 | POST | `/api/ai/oa-fetch-refs` | `routes::api_ai_oa_fetch_refs` | src/common.rs:700 |
| 75 | POST | `/api/ai/oa-panel` | `routes::api_ai_oa_panel` | src/common.rs:701 |
| 76 | POST | `/api/ai/oa-history-compare` | `routes::api_ai_oa_history_compare` | src/common.rs:702 |
| 77 | POST | `/api/ai/oa-examiner-preview` | `routes::api_ai_oa_examiner_preview` | src/common.rs:706 |
| 78 | POST | `/api/ai/oa-defense-analysis` | `routes::api_ai_oa_defense_analysis` | src/common.rs:710 |
| 79 | POST | `/api/ai/oa-strategy-recommend` | `routes::api_ai_oa_strategy_recommend` | src/common.rs:714 |
| 80 | POST | `/api/ai/oa-quality-score` | `routes::api_ai_oa_quality_score` | src/common.rs:718 |
| 81 | POST | `/api/ai/audit-checks` | `routes::api_ai_audit_checks` | src/common.rs:722 |
| 82 | POST | `/api/ai/prohibition-check` | `routes::api_ai_prohibition_check` | src/common.rs:723 |
| 83 | POST | `/api/ai/oa-claim-simulate` | `routes::api_ai_oa_claim_simulate` | src/common.rs:727 |
| 84 | POST | `/api/ai/oa-collect-evidence` | `routes::api_ai_oa_collect_evidence` | src/common.rs:731 |
| 85 | POST | `/api/ai/oa-adversarial` | `routes::api_ai_oa_adversarial` | src/common.rs:735 |
| 86 | POST | `/api/ai/vision-figure-check` | `routes::api_ai_vision_figure_check` | src/common.rs:739 |
| 87 | POST | `/api/ai/oa-feature-matrix` | `routes::api_ai_oa_feature_matrix` | src/common.rs:743 |
| 88 | GET | `/api/oa/history/all` | `routes::api_oa_history_all` | src/common.rs:748 |
| 89 | GET | `/api/oa/history/:patent_number` | `routes::api_oa_history` | src/common.rs:749 |
| 90 | GET | `/api/oa/history/detail/:id` | `routes::api_oa_history_detail` | src/common.rs:753 |
| 91 | POST | `/api/oa/history/:id/delete` | `routes::api_oa_history_delete` | src/common.rs:757 |
| 92 | GET | `/api/oa/history/:patent_number/diff` | `routes::api_oa_history_diff` | src/common.rs:762 |
| 93 | GET | `/api/oa/discussions/:patent_number` | `routes::api_oa_discussion_list` | src/common.rs:767 |
| 94 | GET | `/api/oa/discussions/:patent_number/:discussion_id` | `routes::api_oa_discussion_get` | src/common.rs:771 |
| 95 | POST | `/api/oa/discussions/import` | `routes::api_oa_discussion_import` | src/common.rs:775 |
| 96 | POST | `/api/oa/export-docx` | `routes::api_oa_export_docx` | src/common.rs:780 |
| 97 | POST | `/api/export/docx` | `routes::api_export_markdown_docx` | src/common.rs:781 |
| 98 | GET | `/api/oa/:case_id/export/docx` | `routes::api_oa_case_export_docx` | src/common.rs:782 |
| 99 | POST | `/api/oa/export` | `routes::oa_export::api_oa_export_post` | src/common.rs:787 |
| 100 | GET | `/api/oa/export/:filename` | `routes::oa_export::api_oa_export_get` | src/common.rs:791 |
| 101 | GET | `/api/oa/rounds/:patent_id` | `routes::api_oa_rounds_get` | src/common.rs:796 |
| 102 | GET | `/api/oa/:case_id/rounds/diff` | `routes::api_oa_rounds_diff` | src/common.rs:797 |
| 103 | POST | `/api/oa/rounds` | `routes::api_oa_rounds_create` | src/common.rs:801 |
| 104 | PUT | `/api/oa/rounds/update/:id` | `routes::api_oa_rounds_update` | src/common.rs:802 |
| 105 | GET | `/api/oa/timeline/:patent_id` | `routes::api_oa_timeline` | src/common.rs:806 |
| 106 | POST | `/api/ai/check-amendments` | `routes::api_ai_check_amendments` | src/common.rs:807 |
| 107 | POST | `/api/ai/threat-assessment` | `routes::api_ai_threat_assessment` | src/common.rs:812 |
| 108 | POST | `/api/ai/claim-chart` | `routes::api_ai_claim_chart` | src/common.rs:816 |
| 109 | GET | `/api/ai/cost` | `routes::api_ai_cost_summary` | src/common.rs:817 |
| 110 | GET | `/api/ai/cost/records` | `routes::api_ai_cost_records` | src/common.rs:818 |
| 111 | POST | `/api/ai/cost/record` | `routes::api_ai_cost_save` | src/common.rs:819 |
| 112 | POST | `/api/idea/submit` | `routes::api_idea_submit` | src/common.rs:821 |
| 113 | POST | `/api/idea/decompose` | `routes::api_idea_decompose` | src/common.rs:822 |
| 114 | POST | `/api/idea/analyze` | `routes::api_idea_analyze` | src/common.rs:823 |
| 115 | POST | `/api/idea/pipeline` | `routes::api_idea_pipeline` | src/common.rs:824 |
| 116 | POST | `/api/ideas/batch-compare` | `routes::api_ideas_batch_compare` | src/common.rs:825 |
| 117 | GET | `/api/idea/list` | `routes::api_idea_list` | src/common.rs:829 |
| 118 | GET | `/api/idea/:id` | `routes::api_idea_get` | src/common.rs:830 |
| 119 | POST | `/api/idea/:id/delete` | `routes::api_idea_delete` | src/common.rs:831 |
| 120 | GET | `/api/idea/:id/progress` | `routes::api_idea_progress` | src/common.rs:832 |
| 121 | POST | `/api/idea/:id/resume` | `routes::api_idea_resume` | src/common.rs:833 |
| 122 | GET / POST | `/api/idea/:id/research-state` | `routes::api_idea_research_state` | src/common.rs:834 |
| 123 | POST | `/api/idea/:id/redirect` | `routes::api_idea_redirect` | src/common.rs:838 |
| 124 | GET | `/api/idea/:id/report` | `routes::api_idea_report` | src/common.rs:839 |
| 125 | GET | `/api/idea/:id/report.html` | `routes::api_idea_report_html` | src/common.rs:840 |
| 126 | GET | `/api/ai/models` | `routes::list_ai_models` | src/common.rs:844 |
| 127 | POST | `/api/idea/:id/chat` | `routes::api_idea_chat` | src/common.rs:845 |
| 128 | GET | `/api/idea/:id/evidence` | `routes::api_idea_evidence` | src/common.rs:846 |
| 129 | GET | `/api/idea/:id/kernel-report` | `routes::api_idea_kernel_report` | src/common.rs:847 |
| 130 | GET | `/api/idea/:id/messages` | `routes::api_idea_messages` | src/common.rs:851 |
| 131 | GET | `/api/idea/:id/token-usage` | `routes::api_idea_token_usage` | src/common.rs:852 |
| 132 | GET | `/api/idea/:id/chat/conclusions` | `routes::api_idea_chat_conclusions` | src/common.rs:856 |
| 133 | GET | `/api/idea/:id/chat/conversation` | `routes::api_idea_chat_conversation` | src/common.rs:860 |
| 134 | POST | `/api/idea/:id/summarize` | `routes::api_idea_summarize_discussion` | src/common.rs:864 |
| 135 | GET / POST | `/api/ideas/:id/feature-cards` | `routes::api_get_feature_cards` | src/common.rs:869 |
| 136 | POST | `/api/ideas/:id/feature-cards/generate` | `routes::api_generate_feature_cards` | src/common.rs:873 |
| 137 | GET | `/api/ideas/:id/delivery-pack/xlsx` | `routes::api_idea_delivery_pack_xlsx` | src/common.rs:877 |
| 138 | POST | `/api/ideas/:id/control-prototype` | `routes::api_idea_control_prototype` | src/common.rs:881 |
| 139 | POST | `/api/ideas/:id/fto-scan` | `routes::api_idea_fto_scan` | src/common.rs:885 |
| 140 | GET | `/api/ideas/:id/archive/zip` | `routes::api_idea_archive_zip` | src/common.rs:886 |
| 141 | GET | `/api/ideas/:id/paper/latex-bundle` | `routes::api_idea_paper_latex_bundle` | src/common.rs:890 |
| 142 | GET | `/api/ideas/:id/paper/docx` | `routes::api_idea_paper_docx` | src/common.rs:894 |
| 143 | GET | `/api/feature-cards/diff` | `routes::api_feature_card_diff` | src/common.rs:898 |
| 144 | PUT | `/api/ideas/:id/claims` | `routes::api_idea_claims_replace` | src/common.rs:902 |
| 145 | POST | `/api/ideas/:id/claims/validate` | `routes::api_idea_claims_validate` | src/common.rs:906 |
| 146 | GET / POST | `/api/idea/:id/claim-tree` | `routes::api_idea_claim_tree` | src/common.rs:911 |
| 147 | DELETE | `/api/idea/:id/claim-tree/:claim_id` | `routes::api_idea_claim_node_delete` | src/common.rs:915 |
| 148 | POST | `/api/idea/:id/iterate` | `routes::api_idea_iterate` | src/common.rs:919 |
| 149 | GET | `/api/idea/:id/checkpoints` | `routes::api_idea_checkpoints` | src/common.rs:920 |
| 150 | POST | `/api/idea/:id/checkpoint/rollback` | `routes::api_idea_checkpoint_rollback` | src/common.rs:924 |
| 151 | GET | `/api/idea/:id/versions` | `routes::api_idea_versions` | src/common.rs:928 |
| 152 | GET | `/api/idea/:id/branches` | `routes::api_idea_branches` | src/common.rs:929 |
| 153 | GET | `/api/idea/:id/findings` | `routes::api_idea_findings` | src/common.rs:930 |
| 154 | GET / POST | `/api/idea/:id/memory` | `routes::api_idea_memory` | src/common.rs:932 |
| 155 | DELETE | `/api/idea/:id/memory/:entry_id` | `routes::api_idea_memory_delete` | src/common.rs:936 |
| 156 | GET | `/api/ipc/tree` | `routes::api_ipc_tree` | src/common.rs:941 |
| 157 | GET | `/api/ipc/:code/patents` | `routes::api_ipc_patents` | src/common.rs:942 |
| 158 | POST | `/api/patents/import` | `routes::api_import_patents` | src/common.rs:944 |
| 159 | POST | `/api/patents/import-table` | `routes::api_import_patent_table` | src/common.rs:945 |
| 160 | GET / POST | `/api/patents/sync-folder` | `routes::api_get_patent_sync_folder` | src/common.rs:949 |
| 161 | GET | `/api/patents/sync-folder/progress` | `routes::api_get_patent_sync_progress` | src/common.rs:953 |
| 162 | POST | `/api/patents/sync-folder/cancel` | `routes::api_cancel_patent_sync` | src/common.rs:957 |
| 163 | POST | `/api/backup/export` | `routes::api_backup_export` | src/common.rs:961 |
| 164 | POST | `/api/backup/check` | `routes::api_backup_check` | src/common.rs:962 |
| 165 | GET / POST | `/api/collections` | `routes::api_list_collections` | src/common.rs:964 |
| 166 | DELETE | `/api/collections/:id` | `routes::api_delete_collection` | src/common.rs:968 |
| 167 | GET | `/api/collections/:id/patents` | `routes::api_get_collection_patents` | src/common.rs:972 |
| 168 | POST | `/api/collections/:id/add` | `routes::api_add_to_collection` | src/common.rs:976 |
| 169 | DELETE | `/api/collections/:id/remove/:patent_id` | `routes::api_remove_from_collection` | src/common.rs:980 |
| 170 | GET / POST | `/api/patents/:id/tags` | `routes::api_get_patent_tags` | src/common.rs:985 |
| 171 | DELETE | `/api/patents/:id/tags/:tag` | `routes::api_remove_tag` | src/common.rs:989 |
| 172 | GET | `/api/patents/:id/collections` | `routes::api_get_patent_collections` | src/common.rs:993 |
| 173 | GET | `/api/tags` | `routes::api_list_all_tags` | src/common.rs:997 |
| 174 | POST | `/api/upload/compare` | `routes::api_upload_compare` | src/common.rs:999 |
| 175 | POST | `/api/upload/ingest-local` | `routes::api_upload_ingest_local` | src/common.rs:1000 |
| 176 | POST | `/api/upload/extract` | `routes::api_upload_extract` | src/common.rs:1004 |
| 177 | POST | `/api/upload/extract/stream` | `routes::api_upload_extract_stream` | src/common.rs:1005 |
| 178 | POST | `/api/upload/pdf-store` | `routes::api_upload_pdf_store` | src/common.rs:1009 |
| 179 | GET | `/api/patent/:id/numeral-check` | `routes::api_patent_numeral_check` | src/common.rs:1011 |
| 180 | POST | `/api/ai/numeral-consistency-check` | `routes::api_ai_numeral_consistency_check` | src/common.rs:1015 |
| 181 | GET | `/static/*path` | `serve_static_embedded` | src/common.rs:1022 |
