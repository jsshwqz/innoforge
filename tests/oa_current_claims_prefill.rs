//! #159 的产品侧回归：查到的专利自带现权项时，答复页要把它回填进「现行权利要求书」。
//!
//! 为什么是 DOM 级而不是字符串断言：这条逻辑的价值在「只在空框时回填、并触发 input
//! 让草稿保存接住」。只 grep 源码里有 `current-claims` 证明不了行为，所以这里抽出
//! 模板里真实的 `lookupMyPatent()`，用 mock DOM 跑四种输入组合（见
//! `tests/oa_current_claims_prefill.mjs` 的 CASE 列表）。
//!
//! node 缺失时**失败**而不是跳过：跳过等于把守卫悄悄关掉（与 #59 同口径）。

use std::process::Command;

fn run_script() -> std::process::Output {
    Command::new("node")
        .arg("tests/oa_current_claims_prefill.mjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("无法启动 node；本测试需要 node ≥ 18（CI 的 test job 已安装）")
}

#[test]
fn lookup_prefills_current_claims_only_when_empty() {
    let output = run_script();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "现权项回填的行为断言失败。\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // 逐条确认用例真的跑了（防止脚本提前 return 却 exit 0）
    for case in [
        "CASE prefill_empty PASS",
        "CASE prefill_dispatches_input PASS",
        "CASE existing_value_kept PASS",
        "CASE no_event_when_kept PASS",
        "CASE blank_claims_ignored PASS",
        "CASE no_event_for_blank PASS",
        "CASE missing_claims_ignored PASS",
        "CASE uploaded_my_still_set PASS",
    ] {
        assert!(stdout.contains(case), "缺少用例输出 `{case}`：\n{stdout}");
    }
    assert!(
        stdout.contains("SUMMARY failures=0"),
        "缺少通过汇总行（脚本可能没跑完）：\n{stdout}"
    );
}
