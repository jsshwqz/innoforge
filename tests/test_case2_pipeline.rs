use innoforge::ai::case_input::CaseInput;
use innoforge::ai::response_pipeline::generate_response_letter_pipeline;

#[test]
fn case2_pipeline_body_not_empty() {
    let json = match std::fs::read_to_string("data/uploads/case_input_case2.json") {
        Ok(s) => s,
        Err(_) => {
            eprintln!("skipped: data/uploads/case_input_case2.json not found (gitignored fixture)");
            return;
        }
    };
    let input: CaseInput = serde_json::from_str(&json).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let letter = rt.block_on(generate_response_letter_pipeline(&input, None, None));
    eprintln!("full_text len = {}", letter.full_text.len());
    eprintln!(
        "contains AI事实核查: {}",
        letter.full_text.contains("AI事实核查")
    );
    eprintln!(
        "contains 复审请求书: {}",
        letter.full_text.contains("复审请求书")
    );
    if letter.full_text.len() < 200 {
        eprintln!("FULL TEXT: {}", letter.full_text);
    }
    assert!(
        letter.full_text.contains("复审请求书"),
        "body text should contain 复审请求书"
    );
}
