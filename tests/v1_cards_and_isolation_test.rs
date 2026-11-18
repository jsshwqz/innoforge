//! v1.0 七大独立自治功能卡片端到端契约测试与垂直隔离静态守卫（T-302 ~ T-404）
//!
//! 覆盖：
//! 1. 卡片垂直隔离红线（`AGENTS.md` §〇 公理 1 & §二.2）：`src/cards/*` 各子模块严禁跨卡片相互引用；
//! 2. 首页与 7 张独立卡片页面路由可达性；
//! 3. 卡片 ⑦（`settings_vault`）：四维健康自检（<1.5s）、掩码脱敏配置、原生数据包流式灌库与金库管理；
//! 4. 卡片 ①（`omni_search`）：四维检索、BM25+向量 RRF 段落召回（含父权项链组装）、单篇结构读取与导出；
//! 5. 卡片 ②（`idea_studio`）：多模态共创会话收敛基线卡、三档 DAG 启动、节点分叉（Fork）与实测回填生成 `Exp` 锚点；
//! 6. 卡片 ③（`deep_dissect`）：单篇文献无截断切分入库、结构化深析（特征原子自动沉淀）与局部带图追问；
//! 7. 卡片 ④（`scheme_matrix`）：多方案三维矩阵对决、Hirschberg 差异比对与 `.xlsx` 导出；
//! 8. 卡片 ⑤（`rebuttal_hub`）：三栖抗辩（专利 OA / 论文 Rebuttal）、五维硬门禁核验与修订留痕 `.docx` 导出；
//! 9. 卡片 ⑥（`deliverable_forge`）：先专利后论文公开时序冲突拦截守卫、一源四出编译与导出。

mod common;

use common::sync_http;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const CARD_MODULES: &[&str] = &[
    "omni_search",
    "idea_studio",
    "deep_dissect",
    "scheme_matrix",
    "rebuttal_hub",
    "deliverable_forge",
    "settings_vault",
];

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk_rs_files(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
}

fn scan_cross_card_violations(card_name: &str, source: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        for &other in CARD_MODULES {
            if other == card_name {
                continue;
            }
            let pat_crate = format!("cards::{other}");
            let pat_super = format!("super::super::{other}");
            if trimmed.contains(&pat_crate) || trimmed.contains(&pat_super) {
                violations.push(format!(
                    "line {}: `{}` references sibling card `{other}`",
                    idx + 1,
                    trimmed
                ));
            }
        }
    }
    violations
}

#[test]
fn card_vertical_isolation_guard_zero_cross_card_imports() {
    let cards_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cards");
    assert!(cards_root.exists(), "src/cards directory must exist");

    let mut all_violations = Vec::new();
    for &card in CARD_MODULES {
        let dir = cards_root.join(card);
        assert!(
            dir.exists(),
            "card module directory missing: {}",
            dir.display()
        );
        let mut files = Vec::new();
        walk_rs_files(&dir, &mut files);
        assert!(!files.is_empty(), "card {card} must contain .rs files");

        for file in files {
            let content = fs::read_to_string(&file).expect("read card source");
            for v in scan_cross_card_violations(card, &content) {
                all_violations.push(format!("{}: {v}", file.display()));
            }
        }
    }

    assert!(
        all_violations.is_empty(),
        "AGENTS.md §二.2 violation — cross-card references detected:\n{}",
        all_violations.join("\n")
    );

    // Negative control: ensure scanner catches forbidden sibling card imports
    let synthetic = "use crate::cards::rebuttal_hub::service;\n";
    let caught = scan_cross_card_violations("omni_search", synthetic);
    assert_eq!(
        caught.len(),
        1,
        "negative control must detect cross-card import"
    );
}

#[test]
fn all_seven_independent_cards_end_to_end_contract() {
    let tmp_db = tempfile::NamedTempFile::new().expect("create temp db");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let (port, shutdown_tx, handle) =
        common::spawn_inprocess_with_listener(listener, tmp_db.path().to_str().expect("db path"));
    let client = sync_http::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("build sync_http client");
    let base_url = format!("http://127.0.0.1:{port}");
    let base = &base_url;

    // ── 0. Verify all 7 independent card HTML pages + doc_viewer render 200 OK ──
    for page in [
        "/",
        "/search",
        "/studio",
        "/dissect",
        "/matrix",
        "/rebuttal",
        "/forge",
        "/settings",
        "/doc-viewer",
    ] {
        let resp = client
            .get(format!("{base}{page}"))
            .send()
            .unwrap_or_else(|e| panic!("GET {page} failed: {e}"));
        assert_eq!(resp.status().as_u16(), 200, "page {page} must return 200");
        let html = resp.text().expect("page html");
        assert!(
            html.contains("<!DOCTYPE html>"),
            "page {page} must return full HTML"
        );
    }

    // ── 1. Card ⑦ (`settings_vault`): Health < 1.5s, Masked Config, Native Pack Import, Vault Patch ──
    let t0 = Instant::now();
    let health_resp = client
        .get(format!("{base}/api/settings/health"))
        .send()
        .expect("GET /api/settings/health");
    let elapsed_ms = t0.elapsed().as_millis();
    assert_eq!(health_resp.status().as_u16(), 200);
    assert!(
        elapsed_ms < 1500,
        "T-403 first-run health check must complete under 1500ms, took {elapsed_ms}ms"
    );
    let health_json: serde_json::Value = health_resp.json().expect("health json");
    assert!(health_json.get("overall_status").is_some() || health_json.get("status").is_some());

    let save_cfg_resp = client
        .post(format!("{base}/api/settings/config"))
        .json(&json!({
            "ai_api_key": "sk-synthetic-secret-9988776655",
            "daily_budget": 250
        }))
        .send()
        .expect("POST /api/settings/config");
    assert_eq!(save_cfg_resp.status().as_u16(), 200);

    let cfg_resp = client
        .get(format!("{base}/api/settings/config"))
        .send()
        .expect("GET /api/settings/config");
    assert_eq!(cfg_resp.status().as_u16(), 200);
    let cfg_text = cfg_resp.text().expect("config body");
    assert!(
        !cfg_text.contains("sk-synthetic-secret-9988776655"),
        "masked config snapshot must NEVER echo plaintext API key"
    );

    let pack_json = json!([
        {
            "doc_id": "synth-pat-01",
            "doc_kind": "patent",
            "asset_role": "own_asset",
            "filing_status": "unfiled_secret",
            "external_id": "SYNTH-PAT-2026-001",
            "title_zh": "一种微通道相变液冷散热模组",
            "abstract_text": "本发明公开了一种微通道相变液冷散热模组，通过毛细微槽降低芯片热阻。",
            "claims": "1. 一种微通道相变液冷散热模组，包括基板（101）和微槽盖板（102）。\n2. 根据权利要求1所述的模组，其特征在于，所述微槽盖板（102）内壁设有烧结铜粉层（103）。",
            "fulltext_raw": "[0024] 实施例一：如图1所示，基板（101）与微槽盖板（102）扩散焊连接，烧结铜粉层（103）厚度为0.15mm。"
        },
        {
            "doc_id": "synth-paper-01",
            "doc_kind": "journal_paper",
            "asset_role": "general_ref",
            "filing_status": "public_disclosed",
            "external_id": "10.5555/synth.thermal.2026",
            "doi_verified": true,
            "bibtex_raw": "@article{synth2026thermal, title={Microchannel Two-Phase Cooling}, year={2026}, doi={10.5555/synth.thermal.2026}}",
            "title_en": "Microchannel Two-Phase Cooling for High-Flux Chips",
            "abstract_text": "Demonstrates 38% thermal resistance reduction using sintered micro-grooves."
        }
    ])
    .to_string();

    let import_resp = client
        .post(format!("{base}/api/vault/import-pack"))
        .json(&json!({
            "format": "json",
            "filename": "synth_pack.json",
            "content": pack_json
        }))
        .send()
        .expect("POST /api/vault/import-pack");
    assert_eq!(import_resp.status().as_u16(), 200);
    let import_val: serde_json::Value = import_resp.json().expect("import json");
    assert!(
        import_val["imported_count"].as_u64().unwrap_or(0) >= 2,
        "expected >= 2 imported docs, got: {import_val}"
    );

    let vault_list: serde_json::Value = client
        .get(format!("{base}/api/vault/docs"))
        .send()
        .expect("GET /api/vault/docs")
        .json()
        .expect("vault docs json");
    assert!(vault_list["docs"].as_array().map(|a| a.len()).unwrap_or(0) >= 2);

    // ── 2. Card ① (`omni_search`): Query, Hybrid RRF Chunks, Structured Doc, CSV Export ──
    let omni_q: serde_json::Value = client
        .post(format!("{base}/api/omni-search/query"))
        .json(&json!({
            "query": "微通道相变液冷",
            "realms": ["patent"],
            "enable_ai_rewrite": false
        }))
        .send()
        .expect("POST /api/omni-search/query")
        .json()
        .expect("omni query json");
    assert_eq!(omni_q["status"], "ok");

    let chunks_res: serde_json::Value = client
        .post(format!("{base}/api/omni-search/chunks"))
        .json(&json!({
            "query": "烧结铜粉层",
            "mode": "patent_compare",
            "top_k": 5
        }))
        .send()
        .expect("POST /api/omni-search/chunks")
        .json()
        .expect("omni chunks json");
    assert_eq!(chunks_res["status"], "ok");

    let doc_res: serde_json::Value = client
        .get(format!("{base}/api/omni-search/doc/synth-pat-01"))
        .send()
        .expect("GET /api/omni-search/doc/{id}")
        .json()
        .expect("omni doc json");
    assert_eq!(doc_res["doc"]["doc_id"], "synth-pat-01");

    let csv_resp = client
        .post(format!("{base}/api/omni-search/export"))
        .json(&json!({
            "query": "微通道相变液冷",
            "format": "csv"
        }))
        .send()
        .expect("POST /api/omni-search/export");
    assert_eq!(csv_resp.status().as_u16(), 200);
    assert!(!csv_resp.bytes().expect("csv bytes").is_empty());

    // ── 3. Card ② (`idea_studio`): Session -> Turn -> Tiered Run -> Fork -> Experiment `Exp:` Anchor ──
    let sess_res: serde_json::Value = client
        .post(format!("{base}/api/idea-studio/session"))
        .json(&json!({
            "title": "双相微槽均温板研发",
            "mounted_own_doc_ids": ["synth-pat-01"]
        }))
        .send()
        .expect("POST /api/idea-studio/session")
        .json()
        .expect("session json");
    let session_id = sess_res["session"]["session_id"]
        .as_str()
        .or_else(|| sess_res["session_id"].as_str())
        .expect("session_id present")
        .to_string();

    let turn_res = client
        .post(format!("{base}/api/idea-studio/session/{session_id}/turn"))
        .json(&json!({
            "user_text": "针对高热流GPU热点，在微槽盖板增加梯度孔隙烧结层，目标热阻低于0.08 K/W。",
            "mounted_own_doc_ids": ["synth-pat-01"],
            "lock_baseline": true
        }))
        .send()
        .expect("POST /api/idea-studio/session/{id}/turn");
    assert_eq!(turn_res.status().as_u16(), 200);

    let run_res: serde_json::Value = client
        .post(format!("{base}/api/idea-studio/run/start"))
        .json(&json!({
            "session_id": session_id,
            "tier_mode": "rd_experiment",
            "mounted_own_doc_ids": ["synth-pat-01"]
        }))
        .send()
        .expect("POST /api/idea-studio/run/start")
        .json()
        .expect("run start json");
    let run_id = run_res["run"]["run_id"]
        .as_str()
        .or_else(|| run_res["run_id"].as_str())
        .expect("run_id present")
        .to_string();

    let fork_res: serde_json::Value = client
        .post(format!("{base}/api/idea-studio/run/{run_id}/fork"))
        .json(&json!({
            "fork_from_step": "omni_search",
            "route_label": "路线 B：复合丝网吸液芯"
        }))
        .send()
        .expect("POST /api/idea-studio/run/{id}/fork")
        .json()
        .expect("fork json");
    let child_run_id = fork_res["forked_run"]["run_id"]
        .as_str()
        .or_else(|| fork_res["run"]["run_id"].as_str())
        .or_else(|| fork_res["run_id"].as_str())
        .unwrap_or_else(|| panic!("forked run_id missing in: {fork_res}"));
    assert_ne!(child_run_id, run_id);

    let exp_res: serde_json::Value = client
        .post(format!("{base}/api/idea-studio/run/{run_id}/experiment"))
        .json(&json!({
            "title": "梯度孔隙烧结层热阻对照实验",
            "hypothesis": "梯度孔隙可将极限热流密度提升 25% 以上",
            "control_group_spec": "均匀孔隙 0.15mm 铜粉层",
            "test_group_spec": "双层梯度孔隙（40um/90um）铜粉层",
            "measured_data_json": {"r_th_control": 0.125, "r_th_test": 0.076, "unit": "K/W"},
            "verified": true
        }))
        .send()
        .expect("POST /api/idea-studio/run/{id}/experiment")
        .json()
        .expect("experiment json");
    let exp_anchor = exp_res["evidence_anchor"]
        .as_str()
        .or_else(|| exp_res["anchor_tag"].as_str())
        .unwrap_or("");
    assert!(
        exp_anchor.starts_with("Exp:"),
        "experiment backfill must mint Exp: anchor, got: {exp_res}"
    );

    // ── 4. Card ③ (`deep_dissect`): Ingest + Analyze + Contextual Ask ──
    let dissect_ing: serde_json::Value = client
        .post(format!("{base}/api/deep-dissect/ingest"))
        .json(&json!({
            "doc_id": "synth-dissect-01",
            "doc_kind": "patent",
            "title": "梯度毛细微槽均温装置",
            "fulltext_raw": "权利要求书\n1. 一种梯度毛细微槽均温装置，包括蒸发腔基板（201）和冷凝盖板（202）。\n2. 根据权利要求1所述的装置，其特征在于，所述蒸发腔基板（201）设有回流歧管（203）。\n说明书\n[0024] 如图1所示，蒸发腔基板（201）与冷凝盖板（202）形成闭环回路，回流歧管（203）抑制干涸。",
            "privacy_offline_only": true
        }))
        .send()
        .expect("POST /api/deep-dissect/ingest")
        .json()
        .expect("dissect ingest json");
    assert_eq!(dissect_ing["doc"]["doc_id"], "synth-dissect-01");

    let dissect_ana = client
        .post(format!("{base}/api/deep-dissect/analyze"))
        .json(&json!({
            "doc_id": "synth-dissect-01",
            "mode": "patent",
            "privacy_offline_only": true
        }))
        .send()
        .expect("POST /api/deep-dissect/analyze");
    assert_eq!(dissect_ana.status().as_u16(), 200);

    let dissect_ask = client
        .post(format!("{base}/api/deep-dissect/ask"))
        .json(&json!({
            "doc_id": "synth-dissect-01",
            "question": "回流歧管 203 的作用是什么？",
            "figure_numeral": "203",
            "privacy_offline_only": true
        }))
        .send()
        .expect("POST /api/deep-dissect/ask");
    assert_eq!(dissect_ask.status().as_u16(), 200);

    // ── 5. Card ④ (`scheme_matrix`): 3D Matrix Compare + Get + XLSX Export ──
    let mat_res: serde_json::Value = client
        .post(format!("{base}/api/scheme-matrix/compare"))
        .json(&json!({
            "title": "前代专利 vs 新一代梯度微槽方案对决",
            "mode": "claim_chart",
            "sources": [
                {"label": "我方前代模组", "doc_id": "synth-pat-01"},
                {"label": "新一代回流歧管方案", "doc_id": "synth-dissect-01"}
            ]
        }))
        .send()
        .expect("POST /api/scheme-matrix/compare")
        .json()
        .expect("matrix compare json");
    let matrix_id = mat_res["matrix_id"]
        .as_str()
        .or_else(|| mat_res["matrix"]["matrix_id"].as_str())
        .expect("matrix_id present")
        .to_string();

    let mat_get = client
        .get(format!("{base}/api/scheme-matrix/{matrix_id}"))
        .send()
        .expect("GET /api/scheme-matrix/{id}");
    assert_eq!(mat_get.status().as_u16(), 200);

    let mat_xlsx = client
        .post(format!("{base}/api/scheme-matrix/{matrix_id}/export"))
        .json(&json!({"format": "xlsx"}))
        .send()
        .expect("POST /api/scheme-matrix/{id}/export");
    assert_eq!(mat_xlsx.status().as_u16(), 200);
    let xlsx_bytes = mat_xlsx.bytes().expect("xlsx bytes");
    assert!(
        xlsx_bytes.starts_with(b"PK"),
        "XLSX must be valid OOXML zip"
    );

    // ── 6. Card ⑤ (`rebuttal_hub`): Case + Round + Mock Review + Draft + 5D Verify + Tracked DOCX ──
    let reb_case: serde_json::Value = client
        .post(format!("{base}/api/rebuttal-hub/case"))
        .json(&json!({
            "mode": "patent_oa",
            "title": "微通道散热模组一通答复",
            "own_doc_id": "synth-pat-01",
            "reference_doc_ids": ["synth-dissect-01"],
            "oa_type": "first_oa",
            "dispatch_date": "2026-11-01",
            "original_claim_or_text": "1. 一种微通道相变液冷散热模组，包括基板（101）和微槽盖板（102）。",
            "amended_claim_or_text": "1. 一种微通道相变液冷散热模组，包括基板（101）和微槽盖板（102），其特征在于所述微槽盖板（102）内壁设有厚度为0.15mm的烧结铜粉层（103）。",
            "notice_or_review_text": "审查员认为权利要求1相对于对比文件1缺乏创造性。"
        }))
        .send()
        .expect("POST /api/rebuttal-hub/case")
        .json()
        .expect("rebuttal case json");
    let case_id = reb_case["case"]["case_id"]
        .as_str()
        .or_else(|| reb_case["case_id"].as_str())
        .expect("case_id present")
        .to_string();

    let round_resp = client
        .post(format!("{base}/api/rebuttal-hub/case/{case_id}/round"))
        .json(&json!({
            "round_label": "第一次审查意见答复",
            "oa_type": "first_oa",
            "dispatch_date": "2026-11-01",
            "notice_text": "补充检索对比文件特征核对"
        }))
        .send()
        .expect("POST /api/rebuttal-hub/case/{id}/round");
    assert_eq!(round_resp.status().as_u16(), 200);

    let mock_resp = client
        .post(format!(
            "{base}/api/rebuttal-hub/case/{case_id}/mock-review"
        ))
        .json(&json!({"focus_query": "烧结铜粉层厚度限定是否具备创造性"}))
        .send()
        .expect("POST /api/rebuttal-hub/case/{id}/mock-review");
    assert_eq!(mock_resp.status().as_u16(), 200);

    let draft_resp = client
        .post(format!("{base}/api/rebuttal-hub/case/{case_id}/draft"))
        .json(&json!({
            "preferred_track": "amend_and_argue",
            "user_instruction": "将从属权利要求2并入权利要求1，并引用说明书 [0024] 段。"
        }))
        .send()
        .expect("POST /api/rebuttal-hub/case/{id}/draft");
    assert_eq!(draft_resp.status().as_u16(), 200);

    let verify_resp: serde_json::Value = client
        .post(format!("{base}/api/rebuttal-hub/case/{case_id}/verify"))
        .json(&json!({
            "draft_text": "依据本申请权利要求2（Claim:2）及说明书第 [0024] 段（Desc:[0024]）记载，烧结铜粉层（103）厚度为0.15mm，修改完全未超范围。",
            "require_citations": true
        }))
        .send()
        .expect("POST /api/rebuttal-hub/case/{id}/verify")
        .json()
        .expect("verify json");
    assert!(
        verify_resp.get("gate_report").is_some(),
        "verify response must contain gate_report: {verify_resp}"
    );

    let reb_docx = client
        .post(format!("{base}/api/rebuttal-hub/case/{case_id}/export"))
        .json(&json!({"format": "docx", "acknowledge_priority_warning": true}))
        .send()
        .expect("POST /api/rebuttal-hub/case/{id}/export");
    assert_eq!(reb_docx.status().as_u16(), 200);
    assert!(
        reb_docx
            .bytes()
            .expect("rebuttal docx bytes")
            .starts_with(b"PK"),
        "exported rebuttal package must be valid DOCX/ZIP"
    );

    // ── 7. Card ⑥ (`deliverable_forge`): Priority Guard (Patent-Before-Paper) + Compile + Export ──
    // `synth-pat-01` has `filing_status = unfiled_secret`, so compiling an AcademicPaper MUST trigger priority conflict warning!
    let prio_res: serde_json::Value = client
        .post(format!("{base}/api/deliverable-forge/priority-check"))
        .json(&json!({
            "related_doc_ids": ["synth-pat-01"],
            "target_genre": "academic_paper",
            "knowhow_markers": ["0.15mm"],
            "draft_text": "实验采用 0.15mm 烧结铜粉层实现低热阻。"
        }))
        .send()
        .expect("POST /api/deliverable-forge/priority-check")
        .json()
        .expect("priority check json");
    let lamp = prio_res["priority_check"]["lamp"].as_str().unwrap_or("");
    let has_conflict = prio_res["priority_check"]["has_conflict"]
        .as_bool()
        .unwrap_or(false);
    assert_eq!(
        lamp, "red",
        "Axiom 6 priority guard must flag lamp=red when exporting AcademicPaper before patent filing: {prio_res}"
    );
    assert!(
        has_conflict,
        "Axiom 6 priority guard must set has_conflict=true: {prio_res}"
    );

    let comp_res: serde_json::Value = client
        .post(format!("{base}/api/deliverable-forge/compile"))
        .json(&json!({
            "title": "微通道液冷模组专利技术交底书",
            "genre": "patent_disclosure",
            "related_doc_ids": ["synth-pat-01"],
            "reference_doc_ids": ["synth-paper-01"],
            "use_ai": false
        }))
        .send()
        .expect("POST /api/deliverable-forge/compile")
        .json()
        .expect("forge compile json");
    assert_eq!(comp_res["status"], "ok");

    let forge_tex = client
        .post(format!("{base}/api/deliverable-forge/export"))
        .json(&json!({
            "title": "Microchannel Cooling Manuscript",
            "genre": "academic_paper",
            "format": "tex",
            "sections": [
                ["Introduction", "Recent studies DOI:10.5555/synth.thermal.2026 show promising cooling."]
            ],
            "related_doc_ids": ["synth-pat-01"],
            "reference_doc_ids": ["synth-paper-01"],
            "acknowledge_priority_warning": true,
            "bundle_zip": false
        }))
        .send()
        .expect("POST /api/deliverable-forge/export");
    assert_eq!(forge_tex.status().as_u16(), 200);
    let tex_body = forge_tex.text().expect("tex text");
    assert!(
        tex_body.contains("\\documentclass") && tex_body.contains("10.5555/synth.thermal.2026"),
        "LaTeX export must contain documentclass and verified BibTeX/DOI"
    );

    let _ = shutdown_tx.send(());
    let _ = handle.join();
}
