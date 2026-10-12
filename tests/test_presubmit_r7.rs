use innoforge::ai::case_input::CaseInput;
use innoforge::ai::case_input::{CaseMeta, ReferenceDocument};
use innoforge::ai::pre_submission_check::{
    format_checklist, run_pre_submission_check, TargetSystem,
};
use std::io::Write;

#[test]
fn r7_presubmit_case1_and_simulated_failure() {
    // 输出目录 data/uploads/ 是 gitignore 的本地工作目录，干净检出里不存在。
    // 落盘前必须自建，否则测试在首次运行者的机器上直接崩溃。
    std::fs::create_dir_all("data/uploads").expect("failed to create data/uploads");

    // ── Part 1: Run pre-submission check on case1 ──
    // The fixture file lives in data/uploads/ (gitignored). Skip gracefully if absent.
    let json = match std::fs::read_to_string("data/uploads/case_input_sample.json") {
        Ok(s) => s,
        Err(_) => {
            eprintln!("skipped Part 1: data/uploads/case_input_sample.json not found (gitignored fixture)");
            // Still run Part 2 (simulated failure) which doesn't need the fixture.
            run_simulated_failure();
            return;
        }
    };
    let input: CaseInput = serde_json::from_str(&json).expect("failed to parse case1 input");

    let checklist = run_pre_submission_check(&input, TargetSystem::CnipaReexam);
    let text = format_checklist(&checklist);

    // Save to data/uploads/presubmit_case1.txt
    let mut f = std::fs::File::create("data/uploads/presubmit_case1.txt")
        .expect("failed to create output file");
    f.write_all(text.as_bytes()).expect("failed to write");

    // Verify all 5 required items are present
    assert!(
        text.contains("申请人类型/证件类型"),
        "missing applicant type check"
    );
    assert!(text.contains("地址省市去重"), "missing address dedup check");
    assert!(text.contains("驳回决定发文日"), "missing OA date check");
    assert!(
        text.contains("意见书正文非空"),
        "missing opinion text check"
    );
    assert!(text.contains("附件清单"), "missing attachment list check");

    eprintln!("=== presubmit_case1.txt ===");
    eprintln!("{}", text);

    run_simulated_failure();
}

fn run_simulated_failure() {
    // ── Part 2: Simulated input with empty applicant + duplicated address ──
    let simulated = CaseInput {
        case_meta: CaseMeta {
            application_number: "SIMULATED_FAIL".to_string(),
            applicant: String::new(), // 故意置空申请人类型
            invention_title: "测试发明".to_string(),
            oa_date: Some("2026-07-10".to_string()),
            oa_type: "reject_review".to_string(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "OA正文内容".to_string(),
        reference_documents: vec![ReferenceDocument {
            label: "D1".to_string(),
            publication_number: "CN123456789A".to_string(),
            full_text: "D1原文".to_string(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        }],
        claims_under_review: "权利要求1".to_string(),
        spec_effect_statements: vec![],
        spec_full_text: None,
        legacy_analysis: Some("地址：江苏省江苏省苏州市姑苏区。申请人类型：个人。".to_string()),
        legacy_discussion: None,
        custom_stopwords: vec![],
    };

    let sim_checklist = run_pre_submission_check(&simulated, TargetSystem::CnipaReexam);
    let sim_text = format_checklist(&sim_checklist);

    // Verify red flags are raised
    assert!(!sim_checklist.all_passed, "simulated input should NOT pass");

    let applicant_check = sim_checklist
        .items
        .iter()
        .find(|i| i.name == "申请人类型/证件类型")
        .expect("applicant type check missing");
    assert!(
        !applicant_check.passed,
        "applicant type check must FAIL for empty applicant"
    );

    let addr_check = sim_checklist
        .items
        .iter()
        .find(|i| i.name == "地址省市去重")
        .expect("address check missing");
    assert!(
        !addr_check.passed,
        "address check must FAIL for duplicated province"
    );

    eprintln!("=== simulated failure ===");
    eprintln!("{}", sim_text);

    // Save simulated result too
    let mut f2 = std::fs::File::create("data/uploads/presubmit_simulated_fail.txt")
        .expect("failed to create sim output file");
    f2.write_all(sim_text.as_bytes())
        .expect("failed to write sim");
}
