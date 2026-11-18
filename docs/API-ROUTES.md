# 路由清单（自动生成）

> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。
> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：
> `node scripts/gen_route_inventory.js`
> 逐条「收什么、回什么」的契约（路径参数 / 请求体字段 / 响应键）见 `docs/API-CONTRACTS.md`，同样自动生成。

共 **185** 条路由：页面/静态 16 条，`/api` 接口 169 条。

## 说明：清单只回答「有哪些端点」

清单保证「有哪些端点、处理函数是哪个、在 common.rs 第几行」；字段级契约在 `docs/API-CONTRACTS.md`；
叙述性说明、示例与业务语义在 `docs/API.md`。三份文件分工不同，任何一份都不假装自己覆盖了另外两份。

| # | 方法 | 路径 | 处理函数 | 源码位置 |
| --- | --- | --- | --- | --- |
| 1 | GET | `/api/healthz` | `crate::app::api_healthz` | src/common.rs:394 |
| 2 | POST | `/api/auth/unlock` | `crate::app::api_auth_unlock` | src/common.rs:395 |
| 3 | GET | `/api/settings/health` | `crate::cards::settings_vault::api_settings_health` | src/common.rs:397 |
| 4 | GET / POST | `/api/settings/config` | `crate::cards::settings_vault::api_settings_get_config` | src/common.rs:401 |
| 5 | POST | `/api/settings/probe-provider` | `crate::cards::settings_vault::api_settings_probe_provider` | src/common.rs:406 |
| 6 | POST | `/api/vault/import-pack` | `crate::cards::settings_vault::api_vault_import_pack` | src/common.rs:410 |
| 7 | GET / PATCH / DELETE | `/api/vault/docs` | `crate::cards::settings_vault::api_vault_list_docs` | src/common.rs:414 |
| 8 | POST | `/api/omni-search/query` | `crate::cards::omni_search::api_omni_search_query` | src/common.rs:421 |
| 9 | POST | `/api/omni-search/chunks` | `crate::cards::omni_search::api_omni_search_chunks` | src/common.rs:425 |
| 10 | GET | `/api/omni-search/doc/{id}` | `crate::cards::omni_search::api_omni_search_doc` | src/common.rs:429 |
| 11 | POST | `/api/omni-search/export` | `crate::cards::omni_search::api_omni_search_export` | src/common.rs:433 |
| 12 | POST | `/api/idea-studio/session` | `crate::cards::idea_studio::api_studio_create_session` | src/common.rs:438 |
| 13 | POST | `/api/idea-studio/session/{id}/turn` | `crate::cards::idea_studio::api_studio_turn` | src/common.rs:442 |
| 14 | POST | `/api/idea-studio/run/start` | `crate::cards::idea_studio::api_studio_run_start` | src/common.rs:446 |
| 15 | POST | `/api/idea-studio/run/{id}/fork` | `crate::cards::idea_studio::api_studio_run_fork` | src/common.rs:450 |
| 16 | POST | `/api/idea-studio/run/{id}/checkpoint` | `crate::cards::idea_studio::api_studio_run_checkpoint` | src/common.rs:454 |
| 17 | POST | `/api/idea-studio/run/{id}/experiment` | `crate::cards::idea_studio::api_studio_run_experiment` | src/common.rs:458 |
| 18 | GET | `/api/idea-studio/run/{id}/events` | `crate::cards::idea_studio::api_studio_run_events` | src/common.rs:462 |
| 19 | POST | `/api/deep-dissect/ingest` | `crate::cards::deep_dissect::api_dissect_ingest` | src/common.rs:467 |
| 20 | POST | `/api/deep-dissect/analyze` | `crate::cards::deep_dissect::api_dissect_analyze` | src/common.rs:471 |
| 21 | POST | `/api/deep-dissect/ask` | `crate::cards::deep_dissect::api_dissect_ask` | src/common.rs:475 |
| 22 | POST | `/api/scheme-matrix/compare` | `crate::cards::scheme_matrix::api_matrix_compare` | src/common.rs:480 |
| 23 | GET | `/api/scheme-matrix/{id}` | `crate::cards::scheme_matrix::api_matrix_get` | src/common.rs:484 |
| 24 | POST | `/api/scheme-matrix/{id}/export` | `crate::cards::scheme_matrix::api_matrix_export` | src/common.rs:488 |
| 25 | POST | `/api/rebuttal-hub/case` | `crate::cards::rebuttal_hub::api_rebuttal_create_case` | src/common.rs:493 |
| 26 | POST | `/api/rebuttal-hub/case/{id}/round` | `crate::cards::rebuttal_hub::api_rebuttal_add_round` | src/common.rs:497 |
| 27 | POST | `/api/rebuttal-hub/case/{id}/mock-review` | `crate::cards::rebuttal_hub::api_rebuttal_mock_review` | src/common.rs:501 |
| 28 | POST | `/api/rebuttal-hub/case/{id}/draft` | `crate::cards::rebuttal_hub::api_rebuttal_draft` | src/common.rs:505 |
| 29 | POST | `/api/rebuttal-hub/case/{id}/verify` | `crate::cards::rebuttal_hub::api_rebuttal_verify` | src/common.rs:509 |
| 30 | POST | `/api/rebuttal-hub/case/{id}/export` | `crate::cards::rebuttal_hub::api_rebuttal_export` | src/common.rs:513 |
| 31 | POST | `/api/deliverable-forge/priority-check` | `crate::cards::deliverable_forge::api_forge_priority_check` | src/common.rs:518 |
| 32 | POST | `/api/deliverable-forge/compile` | `crate::cards::deliverable_forge::api_forge_compile` | src/common.rs:522 |
| 33 | POST | `/api/deliverable-forge/export` | `crate::cards::deliverable_forge::api_forge_export` | src/common.rs:526 |
| 34 | POST | `/api/multimodal/upload` | `crate::cards::multimodal_api::api_multimodal_upload` | src/common.rs:531 |
| 35 | GET | `/api/multimodal/task/{id}/events` | `crate::cards::multimodal_api::api_multimodal_task_events` | src/common.rs:535 |
| 36 | GET | `/` | `routes::index_page` | src/common.rs:540 |
| 37 | GET | `/search` | `routes::search_page` | src/common.rs:541 |
| 38 | GET | `/studio` | `routes::studio_page` | src/common.rs:542 |
| 39 | GET | `/dissect` | `routes::dissect_page` | src/common.rs:543 |
| 40 | GET | `/matrix` | `routes::matrix_page` | src/common.rs:544 |
| 41 | GET | `/rebuttal` | `routes::rebuttal_page` | src/common.rs:545 |
| 42 | GET | `/forge` | `routes::forge_page` | src/common.rs:546 |
| 43 | GET | `/doc-viewer` | `routes::doc_viewer_page` | src/common.rs:547 |
| 44 | GET | `/doc/{id}` | `routes::doc_viewer_by_id_page` | src/common.rs:548 |
| 45 | GET | `/patent/{id}` | `routes::patent_detail_page` | src/common.rs:549 |
| 46 | GET | `/ai` | `routes::ai_page` | src/common.rs:550 |
| 47 | GET | `/compare` | `routes::compare_page` | src/common.rs:551 |
| 48 | GET | `/idea` | `routes::idea_page` | src/common.rs:552 |
| 49 | GET | `/settings` | `routes::settings_page` | src/common.rs:553 |
| 50 | GET | `/oa-response` | `routes::office_action_response_page` | src/common.rs:554 |
| 51 | GET | `/api/cad/status` | `routes::api_cad_status` | src/common.rs:556 |
| 52 | POST | `/api/cad/start` | `routes::api_cad_start` | src/common.rs:557 |
| 53 | POST | `/api/cad/draw` | `routes::api_cad_draw` | src/common.rs:558 |
| 54 | GET | `/api/cad/artifacts` | `routes::api_cad_artifacts` | src/common.rs:559 |
| 55 | GET | `/api/cad/artifacts/{id}/preview` | `routes::api_cad_preview` | src/common.rs:560 |
| 56 | GET | `/api/cad/artifacts/{id}/download/{format}` | `routes::api_cad_download` | src/common.rs:564 |
| 57 | GET | `/api/version` | `routes::api_get_version` | src/common.rs:569 |
| 58 | GET | `/api/oa/deadline-table` | `crate::oa_deadline::deadline_table` | src/common.rs:571 |
| 59 | GET | `/api/oa/compute-deadline` | `crate::oa_deadline::compute_deadline_api` | src/common.rs:575 |
| 60 | GET | `/api/settings` | `routes::api_get_settings` | src/common.rs:579 |
| 61 | POST | `/api/settings/cad` | `routes::api_save_cad_settings` | src/common.rs:580 |
| 62 | POST | `/api/settings/search-agent` | `routes::api_save_search_agent_settings` | src/common.rs:581 |
| 63 | POST | `/api/settings/serpapi` | `routes::api_save_serpapi` | src/common.rs:585 |
| 64 | POST | `/api/settings/ai` | `routes::api_save_ai` | src/common.rs:586 |
| 65 | GET | `/api/settings/serpapi/balance` | `routes::api_serpapi_balance` | src/common.rs:587 |
| 66 | GET | `/api/chat/{session_key}` | `routes::api_chat_get_messages` | src/common.rs:592 |
| 67 | POST | `/api/chat/{session_key}/save` | `routes::api_chat_save_message` | src/common.rs:596 |
| 68 | POST | `/api/chat/{session_key}/delete` | `routes::api_chat_delete_messages` | src/common.rs:600 |
| 69 | POST | `/api/search` | `routes::api_search` | src/common.rs:605 |
| 70 | POST | `/api/search/stats` | `routes::api_search_stats` | src/common.rs:606 |
| 71 | POST | `/api/search/export` | `routes::api_export_csv` | src/common.rs:607 |
| 72 | POST | `/api/search/export/xlsx` | `routes::api_export_xlsx` | src/common.rs:608 |
| 73 | POST | `/api/search/online` | `routes::api_search_online` | src/common.rs:609 |
| 74 | POST | `/api/search/vector` | `routes::api_search_vector` | src/common.rs:610 |
| 75 | POST | `/api/search/analyze` | `routes::api_ai_analyze_results` | src/common.rs:611 |
| 76 | POST | `/api/patent/fetch` | `routes::api_fetch_patent` | src/common.rs:613 |
| 77 | GET | `/api/patent/enrich/{id}` | `routes::api_enrich_patent` | src/common.rs:614 |
| 78 | GET | `/api/patent/enrich-free/{id}` | `routes::api_enrich_patent_free` | src/common.rs:615 |
| 79 | GET | `/api/patent/pdf/{id}` | `routes::api_patent_pdf` | src/common.rs:619 |
| 80 | POST | `/api/patent/pdf/extract-text` | `routes::api_patent_pdf_extract_text` | src/common.rs:620 |
| 81 | GET | `/api/patent/image-proxy` | `routes::api_patent_image_proxy` | src/common.rs:624 |
| 82 | GET | `/api/patent/lookup/{number}` | `routes::api_patent_lookup` | src/common.rs:628 |
| 83 | POST | `/api/patent/lookup-or-fetch` | `routes::api_patent_lookup_and_fetch` | src/common.rs:632 |
| 84 | POST | `/api/patent/resolve-chinese-titles` | `routes::api_resolve_chinese_titles` | src/common.rs:636 |
| 85 | GET | `/api/patent/similar/{id}` | `routes::api_recommend_similar` | src/common.rs:640 |
| 86 | GET | `/api/patent/{id}/legal-status` | `routes::api_patent_legal_status` | src/common.rs:644 |
| 87 | POST | `/api/ai/chat` | `routes::api_ai_chat` | src/common.rs:649 |
| 88 | POST | `/api/ai/chat/stream` | `routes::api_ai_chat_stream` | src/common.rs:650 |
| 89 | POST | `/api/ai/chat/conclusions` | `routes::api_ai_chat_conclusions` | src/common.rs:651 |
| 90 | GET | `/api/auth/google/url` | `routes::api_google_auth_url` | src/common.rs:656 |
| 91 | GET | `/api/auth/google/callback` | `routes::api_google_callback` | src/common.rs:657 |
| 92 | POST | `/api/auth/google/exchange` | `routes::api_google_exchange_handler` | src/common.rs:661 |
| 93 | GET | `/api/auth/google/status` | `routes::api_google_oauth_status` | src/common.rs:665 |
| 94 | GET | `/api/auth/gcloud/status` | `routes::api_gcloud_status` | src/common.rs:670 |
| 95 | POST | `/api/auth/gcloud/login` | `routes::api_gcloud_login` | src/common.rs:671 |
| 96 | POST | `/api/ai/summarize` | `routes::api_ai_summarize` | src/common.rs:672 |
| 97 | POST | `/api/ai/compare` | `routes::api_ai_compare` | src/common.rs:673 |
| 98 | POST | `/api/ai/claims` | `routes::api_ai_claims_analysis` | src/common.rs:674 |
| 99 | POST | `/api/ai/risk` | `routes::api_ai_risk_assessment` | src/common.rs:675 |
| 100 | POST | `/api/ai/compare-matrix` | `routes::api_ai_compare_matrix` | src/common.rs:676 |
| 101 | POST | `/api/ai/batch-summarize` | `routes::api_ai_batch_summarize` | src/common.rs:680 |
| 102 | POST | `/api/ai/inventiveness-analysis` | `routes::api_ai_inventiveness_analysis` | src/common.rs:684 |
| 103 | POST | `/api/ai/office-action-response` | `routes::api_ai_office_action_response` | src/common.rs:688 |
| 104 | POST | `/api/ai/office-action-response/stream` | `routes::api_ai_office_action_response_stream` | src/common.rs:692 |
| 105 | POST | `/api/ai/oa-generate-response-letter` | `routes::api_ai_oa_generate_response_letter` | src/common.rs:696 |
| 106 | POST | `/api/ai/oa-discuss` | `routes::api_ai_oa_discuss` | src/common.rs:700 |
| 107 | POST | `/api/ai/oa-fetch-refs` | `routes::api_ai_oa_fetch_refs` | src/common.rs:701 |
| 108 | POST | `/api/ai/oa-panel` | `routes::api_ai_oa_panel` | src/common.rs:702 |
| 109 | POST | `/api/ai/oa-history-compare` | `routes::api_ai_oa_history_compare` | src/common.rs:703 |
| 110 | POST | `/api/ai/oa-examiner-preview` | `routes::api_ai_oa_examiner_preview` | src/common.rs:707 |
| 111 | POST | `/api/ai/oa-defense-analysis` | `routes::api_ai_oa_defense_analysis` | src/common.rs:711 |
| 112 | POST | `/api/ai/oa-strategy-recommend` | `routes::api_ai_oa_strategy_recommend` | src/common.rs:715 |
| 113 | POST | `/api/ai/oa-quality-score` | `routes::api_ai_oa_quality_score` | src/common.rs:719 |
| 114 | POST | `/api/ai/audit-checks` | `routes::api_ai_audit_checks` | src/common.rs:723 |
| 115 | POST | `/api/ai/prohibition-check` | `routes::api_ai_prohibition_check` | src/common.rs:724 |
| 116 | POST | `/api/ai/oa-claim-simulate` | `routes::api_ai_oa_claim_simulate` | src/common.rs:728 |
| 117 | POST | `/api/ai/oa-collect-evidence` | `routes::api_ai_oa_collect_evidence` | src/common.rs:732 |
| 118 | POST | `/api/ai/oa-adversarial` | `routes::api_ai_oa_adversarial` | src/common.rs:736 |
| 119 | GET | `/api/oa/history/all` | `routes::api_oa_history_all` | src/common.rs:741 |
| 120 | GET | `/api/oa/history/{patent_number}` | `routes::api_oa_history` | src/common.rs:742 |
| 121 | GET | `/api/oa/history/detail/{id}` | `routes::api_oa_history_detail` | src/common.rs:746 |
| 122 | POST | `/api/oa/history/{id}/delete` | `routes::api_oa_history_delete` | src/common.rs:750 |
| 123 | GET | `/api/oa/history/{patent_number}/diff` | `routes::api_oa_history_diff` | src/common.rs:755 |
| 124 | GET | `/api/oa/discussions/{patent_number}` | `routes::api_oa_discussion_list` | src/common.rs:760 |
| 125 | GET | `/api/oa/discussions/{patent_number}/{discussion_id}` | `routes::api_oa_discussion_get` | src/common.rs:764 |
| 126 | POST | `/api/oa/discussions/import` | `routes::api_oa_discussion_import` | src/common.rs:768 |
| 127 | POST | `/api/oa/export-docx` | `routes::api_oa_export_docx` | src/common.rs:773 |
| 128 | POST | `/api/oa/export` | `routes::oa_export::api_oa_export_post` | src/common.rs:775 |
| 129 | GET | `/api/oa/export/{filename}` | `routes::oa_export::api_oa_export_get` | src/common.rs:779 |
| 130 | GET | `/api/oa/rounds/{patent_id}` | `routes::api_oa_rounds_get` | src/common.rs:784 |
| 131 | POST | `/api/oa/rounds` | `routes::api_oa_rounds_create` | src/common.rs:785 |
| 132 | PUT | `/api/oa/rounds/update/{id}` | `routes::api_oa_rounds_update` | src/common.rs:786 |
| 133 | GET | `/api/oa/timeline/{patent_id}` | `routes::api_oa_timeline` | src/common.rs:790 |
| 134 | POST | `/api/ai/check-amendments` | `routes::api_ai_check_amendments` | src/common.rs:791 |
| 135 | POST | `/api/ai/threat-assessment` | `routes::api_ai_threat_assessment` | src/common.rs:796 |
| 136 | POST | `/api/ai/claim-chart` | `routes::api_ai_claim_chart` | src/common.rs:800 |
| 137 | GET | `/api/ai/cost` | `routes::api_ai_cost_summary` | src/common.rs:801 |
| 138 | GET | `/api/ai/cost/records` | `routes::api_ai_cost_records` | src/common.rs:802 |
| 139 | POST | `/api/ai/cost/record` | `routes::api_ai_cost_save` | src/common.rs:803 |
| 140 | POST | `/api/idea/submit` | `routes::api_idea_submit` | src/common.rs:805 |
| 141 | POST | `/api/idea/analyze` | `routes::api_idea_analyze` | src/common.rs:806 |
| 142 | POST | `/api/idea/pipeline` | `routes::api_idea_pipeline` | src/common.rs:807 |
| 143 | POST | `/api/ideas/batch-compare` | `routes::api_ideas_batch_compare` | src/common.rs:808 |
| 144 | GET | `/api/idea/list` | `routes::api_idea_list` | src/common.rs:812 |
| 145 | GET | `/api/idea/{id}` | `routes::api_idea_get` | src/common.rs:813 |
| 146 | POST | `/api/idea/{id}/delete` | `routes::api_idea_delete` | src/common.rs:814 |
| 147 | GET | `/api/idea/{id}/progress` | `routes::api_idea_progress` | src/common.rs:815 |
| 148 | POST | `/api/idea/{id}/resume` | `routes::api_idea_resume` | src/common.rs:816 |
| 149 | GET / POST | `/api/idea/{id}/research-state` | `routes::api_idea_research_state` | src/common.rs:817 |
| 150 | POST | `/api/idea/{id}/redirect` | `routes::api_idea_redirect` | src/common.rs:821 |
| 151 | GET | `/api/idea/{id}/report` | `routes::api_idea_report` | src/common.rs:822 |
| 152 | GET | `/api/idea/{id}/report.html` | `routes::api_idea_report_html` | src/common.rs:823 |
| 153 | GET | `/api/ai/models` | `routes::list_ai_models` | src/common.rs:827 |
| 154 | POST | `/api/idea/{id}/chat` | `routes::api_idea_chat` | src/common.rs:828 |
| 155 | GET | `/api/idea/{id}/evidence` | `routes::api_idea_evidence` | src/common.rs:829 |
| 156 | GET | `/api/idea/{id}/kernel-report` | `routes::api_idea_kernel_report` | src/common.rs:830 |
| 157 | GET | `/api/idea/{id}/messages` | `routes::api_idea_messages` | src/common.rs:834 |
| 158 | GET | `/api/idea/{id}/chat/conclusions` | `routes::api_idea_chat_conclusions` | src/common.rs:835 |
| 159 | GET | `/api/idea/{id}/chat/conversation` | `routes::api_idea_chat_conversation` | src/common.rs:839 |
| 160 | POST | `/api/idea/{id}/summarize` | `routes::api_idea_summarize_discussion` | src/common.rs:843 |
| 161 | GET / POST | `/api/ideas/{id}/feature-cards` | `routes::api_get_feature_cards` | src/common.rs:848 |
| 162 | GET | `/api/feature-cards/diff` | `routes::api_feature_card_diff` | src/common.rs:852 |
| 163 | GET | `/api/idea/{id}/claim-tree` | `routes::api_idea_claim_tree` | src/common.rs:857 |
| 164 | POST | `/api/idea/{id}/iterate` | `routes::api_idea_iterate` | src/common.rs:861 |
| 165 | GET | `/api/idea/{id}/versions` | `routes::api_idea_versions` | src/common.rs:862 |
| 166 | GET | `/api/idea/{id}/branches` | `routes::api_idea_branches` | src/common.rs:863 |
| 167 | GET | `/api/idea/{id}/findings` | `routes::api_idea_findings` | src/common.rs:864 |
| 168 | GET / POST | `/api/idea/{id}/memory` | `routes::api_idea_memory` | src/common.rs:866 |
| 169 | DELETE | `/api/idea/{id}/memory/{entry_id}` | `routes::api_idea_memory_delete` | src/common.rs:870 |
| 170 | GET | `/api/ipc/tree` | `routes::api_ipc_tree` | src/common.rs:875 |
| 171 | GET | `/api/ipc/{code}/patents` | `routes::api_ipc_patents` | src/common.rs:876 |
| 172 | POST | `/api/patents/import` | `routes::api_import_patents` | src/common.rs:878 |
| 173 | GET / POST | `/api/collections` | `routes::api_list_collections` | src/common.rs:880 |
| 174 | DELETE | `/api/collections/{id}` | `routes::api_delete_collection` | src/common.rs:884 |
| 175 | GET | `/api/collections/{id}/patents` | `routes::api_get_collection_patents` | src/common.rs:888 |
| 176 | POST | `/api/collections/{id}/add` | `routes::api_add_to_collection` | src/common.rs:892 |
| 177 | DELETE | `/api/collections/{id}/remove/{patent_id}` | `routes::api_remove_from_collection` | src/common.rs:896 |
| 178 | GET / POST | `/api/patents/{id}/tags` | `routes::api_get_patent_tags` | src/common.rs:901 |
| 179 | DELETE | `/api/patents/{id}/tags/{tag}` | `routes::api_remove_tag` | src/common.rs:905 |
| 180 | GET | `/api/patents/{id}/collections` | `routes::api_get_patent_collections` | src/common.rs:909 |
| 181 | GET | `/api/tags` | `routes::api_list_all_tags` | src/common.rs:913 |
| 182 | POST | `/api/upload/compare` | `routes::api_upload_compare` | src/common.rs:915 |
| 183 | POST | `/api/upload/extract` | `routes::api_upload_extract` | src/common.rs:916 |
| 184 | POST | `/api/upload/pdf-store` | `routes::api_upload_pdf_store` | src/common.rs:917 |
| 185 | GET | `/static/{*path}` | `serve_static_embedded` | src/common.rs:921 |
