const CAD_SOURCE: &str = include_str!("../static/cad.js");
const CAD_RUST_SOURCE: &str = include_str!("../src/cad.rs");

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
    assert!(CAD_SOURCE.contains("request.prompt"));
}

#[test]
fn bootstrap_timeout_kills_the_owned_child_process() {
    assert!(CAD_RUST_SOURCE.contains("kill_on_drop(true)"));
    assert!(CAD_RUST_SOURCE.contains("STARTUP_RETRY_COOLDOWN"));
    assert!(CAD_RUST_SOURCE.contains("FreeCAD is still starting"));
}

#[test]
fn retry_uses_the_failed_request_snapshot_and_history_can_be_rendered_again() {
    assert!(CAD_SOURCE.contains("retryRequest"));
    assert!(CAD_SOURCE.contains("renderHistory"));
    assert!(!CAD_SOURCE.contains("draw(state.originalPrompt, state.latest && state.latest.id)"));
}
