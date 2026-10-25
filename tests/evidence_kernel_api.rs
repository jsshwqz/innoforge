//! GET /api/idea/:id/kernel-report 集成测试（EK-v2 结构化报告暴露）。
//!
//! 四种明确状态逐一验证：error（创意不存在）/ no_snapshot / no_kernel_report（旧快照）/
//! ok（带完整内核报告）。快照通过直插 DB 构造（WAL 并发安全），不走重型 pipeline。
//!
//! 全程同步（harness 就绪探测与断言请求均用 reqwest::blocking）——
//! blocking reqwest 内部自建 runtime，放进异步上下文会 panic，故不用 #[tokio::test]。

mod common;

use serde_json::Value;

fn db_path(home: &std::path::Path) -> std::path::PathBuf {
    home.join("innoforge.db")
}

fn insert_idea(home: &std::path::Path, idea_id: &str) {
    let conn = rusqlite::Connection::open(db_path(home)).expect("open db");
    conn.execute(
        "INSERT INTO ideas (id, title, description) VALUES (?1, ?2, ?3)",
        rusqlite::params![idea_id, "温控创意", "描述"],
    )
    .expect("insert idea");
}

fn insert_snapshot(home: &std::path::Path, idea_id: &str, kernel_report: Option<String>) {
    let mut ctx = innoforge::pipeline::context::PipelineContext::new(idea_id, "温控创意", "描述");
    ctx.kernel_report = kernel_report;
    let context_json = serde_json::to_string(&ctx).expect("serialize ctx");
    let conn = rusqlite::Connection::open(db_path(home)).expect("open db");
    conn.execute(
        "INSERT OR REPLACE INTO pipeline_snapshots (idea_id, context_json, current_step)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![idea_id, context_json, "Finalize"],
    )
    .expect("insert snapshot");
}

fn get_report(port: u16, idea_id: &str) -> Value {
    let url = format!("http://127.0.0.1:{port}/api/idea/{idea_id}/kernel-report");
    reqwest::blocking::get(&url)
        .expect("http get")
        .json::<Value>()
        .expect("json body")
}

#[test]
fn kernel_report_endpoint_states() {
    let port = common::free_port();
    let ai_port = common::free_port();
    let home = tempfile::tempdir().expect("home dir");
    // harness 会拿走 TempDir 所有权（存活期与 server 绑定），先克隆路径供直插 DB 用
    let home_path = home.path().to_path_buf();
    let _server = common::spawn_test_server_with_timeout(
        port,
        60,
        home,
        &[
            ("AI_BASE_URL", &format!("http://127.0.0.1:{ai_port}")),
            ("AI_API_KEY", "test-key-not-real"),
        ],
    );

    // 1) 创意不存在 → error
    let v = get_report(port, "no-such-idea");
    assert_eq!(v["status"], "error", "{v}");

    // 2) 创意在、无快照 → no_snapshot（不伪装成通过）
    insert_idea(&home_path, "idea-kr");
    let v = get_report(port, "idea-kr");
    assert_eq!(v["status"], "no_snapshot", "{v}");

    // 3) 旧快照（无 kernel_report 字段）→ no_kernel_report
    insert_snapshot(&home_path, "idea-kr", None);
    let v = get_report(port, "idea-kr");
    assert_eq!(v["status"], "no_kernel_report", "{v}");
    assert_eq!(v["current_step"], "Finalize", "{v}");

    // 4) 带内核报告 → ok，结构化字段齐全（引用编号与 corpus_from_chunks 的 引用N 对齐）
    let (kr, _) = innoforge::evidence_kernel::run_kernel(
        "本申请记载了“当温度超过阈值时，控制器自动启动风扇并根据温差调节转速”[引用1]。",
        &innoforge::evidence_kernel::corpus_from_chunks(&[
            innoforge::pipeline::context::ReferenceChunk {
                id: "c1".into(),
                patent_id: "P1".into(),
                chunk_index: 0,
                source_type: "description".into(),
                content: "当温度超过阈值时，控制器自动启动风扇并根据温差调节转速。".into(),
                relevance_score: 0.9,
            },
        ]),
    );
    let kr_json = serde_json::to_string(&kr).expect("serialize kernel report");
    insert_snapshot(&home_path, "idea-kr", Some(kr_json));
    let v = get_report(port, "idea-kr");
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["kernel_report"]["status"], "ok", "{v}");
    assert!(v["kernel_report"]["rubric"]["grounding"].is_number(), "{v}");
    assert!(
        v["kernel_report"]["claims"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "{v}"
    );
}
