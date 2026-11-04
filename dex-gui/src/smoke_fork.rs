// Modified by Jatin Bharati (2026-11-04): new module, browser tests for the fork's page (fork-webapp). See NOTICE.
//! Browser tests (`just test-wasm`, inside `nix develop .#wasm-test`) for the fork's arrival page and
//! its SDK report. `smoke.rs` stays as upstream wrote it.

use dexpa_sdk::{
    catalogue,
    scan::{read_input, report_for},
};
use leptos::{mount::mount_to, prelude::*, wasm_bindgen::JsCast, web_sys::HtmlElement};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use crate::{
    load::{GUI_LIMITS, SAMPLE_APK},
    ui::App,
};

wasm_bindgen_test_configure!(run_in_browser);

/// Mounts the app into a fresh container of its own.
fn mounted() -> HtmlElement {
    let container = document().create_element("div").expect("create a div");
    document()
        .body()
        .expect("a body")
        .append_child(&container)
        .expect("append");
    let container: HtmlElement = container.unchecked_into();
    mount_to(container.clone(), || view! { <App/> }).forget();
    container
}

#[wasm_bindgen_test]
fn arrival_shows_the_fork_and_the_original() {
    let text = mounted().text_content().unwrap_or_default();
    for expected in [
        "See what's inside an Android app",
        "J18Bharati",
        "dhilipsiva",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in: {text}");
    }
}

#[wasm_bindgen_test]
fn file_picker_takes_apk_and_dex() {
    let input = mounted()
        .query_selector("input[type=file]")
        .expect("valid selector")
        .expect("the file picker is missing");
    let accept = input.get_attribute("accept").unwrap_or_default();
    assert!(accept.contains(".apk"), "accept = {accept:?}");
    assert!(accept.contains(".dex"), "accept = {accept:?}");
}

#[wasm_bindgen_test]
fn sample_report_matches_the_cli_golden() {
    let input = read_input(SAMPLE_APK, GUI_LIMITS).expect("the sample reads");
    let report = report_for(&catalogue::bundled(), &input).expect("the sample scans");
    let got = serde_json::to_value(&report).expect("serialises");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../dex-cli/tests/golden/sdks-positive-apk.json"
    ))
    .expect("golden is JSON");
    assert_eq!(got, golden);
}

#[wasm_bindgen_test]
async fn the_sample_app_loads_and_the_picker_is_free_again() {
    let page = mounted();
    let button: HtmlElement = page
        .query_selector("button.ghost")
        .expect("valid selector")
        .expect("the sample button is missing")
        .unchecked_into();
    button.click();
    let mut text = String::new();
    for _ in 0..100 {
        let tick = js_sys::Promise::new(&mut |resolve, _| {
            let _ = document()
                .default_view()
                .expect("a window")
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
        });
        JsFuture::from(tick).await.expect("timer");
        text = page.text_content().unwrap_or_default();
        if text.contains("Google Mobile Ads")
            && text.contains("Classes")
            && !text.contains("Working")
        {
            break;
        }
    }
    assert!(text.contains("Google Mobile Ads"), "the SDK list: {text}");
    assert!(
        !text.contains("See what's inside"),
        "the arrival screen stayed: {text}"
    );
    let input = page
        .query_selector("input[type=file]")
        .expect("valid selector")
        .expect("the picker is missing");
    assert!(input.get_attribute("disabled").is_none(), "still busy");
}
