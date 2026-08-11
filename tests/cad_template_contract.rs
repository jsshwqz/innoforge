const IDEA: &str = include_str!("../templates/idea.html");
const PATENT: &str = include_str!("../templates/patent_detail.html");
const OA: &str = include_str!("../templates/office_action_response.html");
const SETTINGS: &str = include_str!("../templates/settings.html");

#[test]
fn all_conversation_pages_load_the_shared_cad_controller_and_button() {
    for page in [IDEA, PATENT, OA] {
        let i18n = page.find("/static/i18n.js").expect("i18n script");
        let cad = page.find("/static/cad.js").expect("CAD script");
        assert!(i18n < cad);
        assert!(page.contains("data-i18n=\"cad.draw\""));
        assert!(page.contains("shouldHandle"));
        assert!(page.contains("drawFromInput"));
    }
}

#[test]
fn settings_can_detect_and_start_freecad() {
    assert!(SETTINGS.contains("id=\"freecad-status\""));
    assert!(SETTINGS.contains("checkAndStartFreeCad"));
    assert!(SETTINGS.contains("/api/cad/status"));
}
