//! G7: Deployment E2E scan verification (issue #30).
//!
//! Verifies the artifact saved in docs/r19-e2e-artifact.json:
//! 1. The JSON artifact is valid and contains expected fields
//! 2. The scan document exists and contains key findings
//! 3. The e2e result has expected total count of 87

use std::fs;

#[test]
fn g7_e2e_artifact_json_is_valid() {
    let content =
        fs::read_to_string("docs/r19-e2e-artifact.json").expect("r19-e2e-artifact.json must exist");

    // Verify it's valid JSON
    let v: serde_json::Value =
        serde_json::from_str(&content).expect("r19-e2e-artifact.json must be valid JSON");

    // Check required fields
    assert!(v["scan_date"].is_string(), "scan_date must be present");
    assert!(
        v["deployment"]["url"].is_string(),
        "deployment.url must be present"
    );
    assert!(
        v["deployment"]["git_commit"].is_string(),
        "deployment.git_commit must be present"
    );
    assert!(
        v["e2e_result"]["total"].is_number(),
        "e2e_result.total must be present"
    );
    assert!(
        v["e2e_result"]["passed"].is_number(),
        "e2e_result.passed must be present"
    );
    assert!(
        v["e2e_result"]["failed"].is_number(),
        "e2e_result.failed must be present"
    );
    assert!(
        v["templates"].is_object(),
        "templates with sha256 must be present"
    );
}

#[test]
fn g7_e2e_result_total_is_87() {
    let content = fs::read_to_string("docs/r19-e2e-artifact.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let total = v["e2e_result"]["total"].as_i64().unwrap();
    assert_eq!(total, 87, "e2e test total must be 87");

    let passed = v["e2e_result"]["passed"].as_i64().unwrap();
    assert_eq!(passed, 84, "e2e passed must be 84");

    let failed = v["e2e_result"]["failed"].as_i64().unwrap();
    assert_eq!(failed, 3, "e2e failed must be 3");

    let skipped = v["e2e_result"]["skipped"].as_i64().unwrap();
    assert_eq!(skipped, 2, "e2e skipped must be 2");
}

#[test]
fn g7_deployment_scan_document_exists() {
    let content = fs::read_to_string("docs/r19-deployment-scan.md")
        .expect("r19-deployment-scan.md must exist");

    // Verify key sections are present
    assert!(
        content.contains("## E2E Test Result"),
        "scan doc must contain E2E Test Result section"
    );
    assert!(
        content.contains("## Failures"),
        "scan doc must contain Failures section"
    );
    assert!(
        content.contains("## Skips"),
        "scan doc must contain Skips section"
    );
    assert!(
        content.contains("84/87"),
        "scan doc must contain the 84/87 passed count"
    );
    // Each failure must be explained
    assert!(
        content.contains("Root cause"),
        "scan doc must explain root causes for failures"
    );
}

#[test]
fn g7_template_sha256_recorded() {
    let content = fs::read_to_string("docs/r19-e2e-artifact.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let templates = v["templates"].as_object().unwrap();
    for (name, tmpl) in templates {
        let sha = tmpl["sha256"].as_str().unwrap_or("");
        assert!(
            sha.len() == 64,
            "Template {} sha256 must be 64 hex chars, got {}",
            name,
            sha.len()
        );
        // Verify it's hex
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "Template {} sha256 must be hex",
            name
        );
    }
}

#[test]
fn g7_git_commit_recorded() {
    let content = fs::read_to_string("docs/r19-e2e-artifact.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    let commit = v["deployment"]["git_commit"].as_str().unwrap_or("");
    assert!(
        commit.len() >= 40,
        "git_commit must be full SHA (40+ chars), got {}",
        commit.len()
    );
}
