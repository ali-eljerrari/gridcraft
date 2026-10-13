//! Sheet-tab layout and splitter behavior (#943).

use egui::vec2;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

fn harness() -> egui_kittest::Harness<'static, SheetApp> {
    let mut session = Session::new();
    session.new_workbook();
    session.execute("home.insertSheet", json!({"name": "Sheet2"})).unwrap();
    egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        SheetApp::new(session, Default::default()),
    )
}

#[test]
fn new_sheet_button_follows_last_tab_when_tabs_fit() {
    let mut h = harness();
    h.run_steps(5);
    let geometry = h.state().tab_geometry;
    let last = geometry.last_tab.expect("visible sheet tab geometry");
    let plus = geometry.new_sheet.expect("new sheet button geometry");
    assert!((plus.left() - last.right()).abs() <= 8.0, "plus should follow the last tab: tab={last:?}, plus={plus:?}");
}
