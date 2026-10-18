//! End-to-end regression for issue #70:
//! 全新空库 + 未配搜索（无 AI key、无 SerpAPI）点「快速推演」(POST /api/idea/analyze)
//! 修复前返回 novelty_score:100.0 的假满分结论；修复后必须返回 novelty_score:null
//! 且 novelty_status:"insufficient_evidence"。
//!
//! 回归入口：端到端 submit → analyze，不只测 execute(&mut ctx) 内部函数。
//! 判据通用：输入是公开通用技术描述，无案件信息。

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

static DB_TEMPLATE: OnceLock<PathBuf> = OnceLock::new();

fn ensure_db_template() -> PathBuf {
    DB_TEMPLATE
        .get_or_init(|| {
            let template_dir = tempfile::tempdir().expect("create template dir");
            let template_port = free_port();
            let child = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
                .env("INNOFORGE_PORT", template_port.to_string())
                .env("INNOFORGE_NO_OPEN", "1")
                .env("INNOFORGE_HOME", template_dir.path().to_str().unwrap())
                .spawn()
                .expect("spawn server for template");
            assert!(wait_for_server(template_port, 60));
            kill_server(child);
            let db_path = template_dir.path().join("innoforge.db");
            assert!(db_path.exists(), "template db must exist after migration");
            std::mem::forget(template_dir);
            db_path
        })
        .clone()
}

fn fresh_db_dir() -> tempfile::TempDir {
    let template = ensure_db_template();
    let dir = tempfile::tempdir().expect("create fresh db dir");
    std::fs::copy(&template, dir.path().join("innoforge.db")).expect("copy template db");
    dir
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().unwrap().port()
}

fn wait_for_server(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if let Ok(resp) =
            reqwest::blocking::get(format!("http://127.0.0.1:{}/api/version", port))
        {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

fn kill_server(mut child: std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// 正向 A（#70 验收·缺陷必须消失）：空库 + 未配搜索 ⇒ novelty_score null + insufficient_evidence。
#[test]
fn empty_evidence_must_not_claim_novelty_score() {
    let port = free_port();
    let home = fresh_db_dir();

    // 不设 AI_API_KEY / AI_BASE_URL / AI_MODEL / SerpAPI —— 全新空库、未配搜索
    let server = Command::new(env!("CARGO_BIN_EXE_innoforge-server"))
        .env("INNOFORGE_PORT", port.to_string())
        .env("INNOFORGE_NO_OPEN", "1")
        .env("INNOFORGE_HOME", home.path().to_str().unwrap())
        .env("AI_API_KEY", "")
        .env_remove("AI_BASE_URL")
        .env_remove("AI_MODEL")
        .env_remove("SERPAPI_KEY")
        .env_remove("SERPAPI_API_KEY")
        .spawn()
        .expect("spawn server");
    assert!(wait_for_server(port, 60), "server must start");

    let client = reqwest::blocking::Client::new();

    // 1) submit
    let submit_resp = client
        .post(format!("http://127.0.0.1:{}/api/idea/submit", port))
        .header("Content-Type", "application/json")
        .body(r#"{"title":"相变储热散热底座","input_type":"text"}"#)
        .send()
        .expect("submit must respond");
    assert!(submit_resp.status().is_success(), "submit status");
    let submit_json: serde_json::Value = submit_resp.json().expect("submit json");
    let id = submit_json["id"]
        .as_str()
        .or_else(|| submit_json["idea"]["id"].as_str())
        .expect("submit must return id")
        .to_string();

    // 2) analyze (quick)
    let analyze_resp = client
        .post(format!("http://127.0.0.1:{}/api/idea/analyze", port))
        .header("Content-Type", "application/json")
        .body(format!(r#"{{"id":"{}"}}"#, id))
        .send()
        .expect("analyze must respond");
    assert!(analyze_resp.status().is_success(), "analyze status");
    let analyze_json: serde_json::Value = analyze_resp.json().expect("analyze json");

    let idea = &analyze_json["idea"];
    assert!(
        idea["novelty_score"].is_null(),
        "#70: 空证据下 novelty_score 必须为 null，实际={}",
        idea["novelty_score"]
    );
    assert_eq!(
        idea["novelty_status"], "insufficient_evidence",
        "#70: 空证据下 novelty_status 必须标 insufficient_evidence"
    );
    // 文案不得再出现满分结论
    let analysis = idea["analysis"].as_str().unwrap_or("");
    assert!(
        !analysis.contains("100/100") && !analysis.contains("评分：100"),
        "#70: 空证据报告不得给满分结论，analysis={}",
        analysis
    );
    assert!(
        analysis.contains("证据不足") || analysis.contains("无法评估"),
        "#70: 空证据报告应说明证据不足/无法评估，analysis={}",
        analysis
    );

    kill_server(server);
}
