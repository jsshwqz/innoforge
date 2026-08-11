const CAD_SOURCE: &str = include_str!("../static/cad.js");

#[test]
fn shared_cad_controller_is_safe_and_complete() {
    assert!(CAD_SOURCE.contains("window.InnoForgeCad"));
    assert!(CAD_SOURCE.contains("createController"));
    assert!(CAD_SOURCE.contains("shouldHandle"));
    assert!(CAD_SOURCE.contains("drawFromInput"));
    assert!(CAD_SOURCE.contains("createElement"));
    assert!(CAD_SOURCE.contains("textContent"));
    assert!(!CAD_SOURCE.contains(".innerHTML"));
    for action in ["Continue modifying", "Fullscreen", "FCStd", "STEP"] {
        assert!(CAD_SOURCE.contains(action));
    }
    assert!(CAD_SOURCE.contains("originalPrompt"));
}
