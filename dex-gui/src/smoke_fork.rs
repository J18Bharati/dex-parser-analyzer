// Modified by Jatin Bharati (2026-11-04): new module, browser tests for the fork's page (fork-webapp). See NOTICE.
//! Browser tests (`just test-wasm`, inside `nix develop .#wasm-test`) for the fork's arrival page and
//! its SDK report. `smoke.rs` stays as upstream wrote it.

use std::sync::Arc;

use dexpa_core::{axml::XmlValue, manifest::Manifest};
use dexpa_sdk::{
    catalogue,
    scan::{read_input, report_for},
};
use leptos::{mount::mount_to, prelude::*, wasm_bindgen::JsCast, web_sys::HtmlElement};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use crate::{
    load::{ClassRow, ClassRows, GUI_LIMITS, SAMPLE_APK, Section},
    ui::{
        App,
        dex_view::ClassesView,
        manifest_view::ManifestView,
        results::Results,
        sdk_view::SdkView,
        state::{AppSummary, LoadState},
    },
};

const BARE_DEX: &[u8] = include_bytes!("../../dex-analysis/tests/data/rules-positive.dex");

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

/// Mounts `view` into a fresh container and returns it.
fn mount_view<F, V>(view: F) -> HtmlElement
where
    F: FnOnce() -> V + 'static,
    V: IntoView + 'static,
{
    let container = document().create_element("div").expect("create a div");
    document()
        .body()
        .expect("a body")
        .append_child(&container)
        .expect("append");
    let container: HtmlElement = container.unchecked_into();
    mount_to(container.clone(), view).forget();
    container
}

/// Waits `ms` milliseconds, so queued reactive updates and timers can run.
async fn wait(ms: i32) {
    let tick = js_sys::Promise::new(&mut |resolve, _| {
        let _ = document()
            .default_view()
            .expect("a window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    JsFuture::from(tick).await.expect("timer");
}

/// Waits until `done` holds, for up to five seconds.
async fn until(mut done: impl FnMut() -> bool) {
    for _ in 0..100 {
        wait(50).await;
        if done() {
            return;
        }
    }
}

fn sample_report() -> Arc<dexpa_sdk::report::SdkReport> {
    let input = read_input(SAMPLE_APK, GUI_LIMITS).expect("the sample reads");
    Arc::new(report_for(&catalogue::bundled(), &input).expect("the sample scans"))
}

fn class_rows(n: usize) -> Arc<ClassRows> {
    Arc::new(ClassRows {
        rows: (0..n)
            .map(|i| ClassRow {
                descriptor: format!("Lcom/example/Class{i};"),
                method_count: 1,
                dex_index: 0,
            })
            .collect(),
        unreadable: 0,
    })
}

fn text_of(element: &HtmlElement) -> String {
    element.text_content().unwrap_or_default()
}

fn card_class(page: &HtmlElement, name: &str) -> String {
    let cards = page.query_selector_all("li.sdk").expect("valid selector");
    (0..cards.length())
        .filter_map(|i| cards.item(i))
        .find(|card| card.text_content().unwrap_or_default().contains(name))
        .and_then(|card| card.dyn_into::<HtmlElement>().ok())
        .unwrap_or_else(|| panic!("no card for {name}"))
        .class_name()
}

#[wasm_bindgen_test]
fn sdk_view_renders_evidence_and_the_caveat() {
    let report = sample_report();
    let page = mount_view(move || view! { <SdkView report=report/> });
    let text = text_of(&page);
    for expected in ["Sentry", "io.sentry.dsn", "doesn't prove", "meta-data"] {
        assert!(text.contains(expected), "missing {expected:?} in: {text}");
    }
}

#[wasm_bindgen_test]
fn tracking_sdks_are_amber() {
    let report = sample_report();
    let page = mount_view(move || view! { <SdkView report=report/> });
    assert!(card_class(&page, "Google Mobile Ads").contains("warn"));
    assert!(!card_class(&page, "Sentry").contains("warn"));
}

#[wasm_bindgen_test]
fn manifest_view_explains_a_bare_dex() {
    let page = mount_view(|| view! { <ManifestView manifest=None error=None/> });
    assert!(text_of(&page).contains("No manifest: this was a DEX file, not an APK."));
}

#[wasm_bindgen_test]
fn manifest_view_shows_a_reference_version_as_is() {
    let manifest = Manifest {
        package: Some("app.example".to_owned()),
        version_name: Some(XmlValue::Reference(0x7f0e_0012)),
        ..Manifest::default()
    };
    let page =
        mount_view(move || view! { <ManifestView manifest=Some(Arc::new(manifest)) error=None/> });
    assert!(text_of(&page).contains("@0x7f0e0012"));
}

#[wasm_bindgen_test]
fn classes_view_caps_rows() {
    let classes = class_rows(501);
    let page = mount_view(move || view! { <ClassesView classes=classes/> });
    let rows = page.query_selector_all("li.class-row").expect("selector");
    assert_eq!(rows.length(), 500);
    assert!(text_of(&page).contains("showing 500 of 501"));
}

#[wasm_bindgen_test]
async fn classes_view_filter_ignores_case() {
    let classes = Arc::new(ClassRows {
        rows: vec![
            ClassRow {
                descriptor: "Lio/sentry/Hub;".to_owned(),
                method_count: 2,
                dex_index: 0,
            },
            ClassRow {
                descriptor: "Lcom/example/Main;".to_owned(),
                method_count: 1,
                dex_index: 0,
            },
        ],
        unreadable: 0,
    });
    let page = mount_view(move || view! { <ClassesView classes=classes/> });
    let input: leptos::web_sys::HtmlInputElement = page
        .query_selector("input.filter")
        .expect("selector")
        .expect("the filter is missing")
        .unchecked_into();
    input.set_value("SENTRY");
    input
        .dispatch_event(&leptos::web_sys::Event::new("input").expect("event"))
        .expect("dispatch");
    wait(50).await;
    let text = text_of(&page);
    assert!(text.contains("io/sentry/Hub"), "{text}");
    assert!(!text.contains("com/example/Main"), "{text}");
}

#[wasm_bindgen_test]
async fn unavailable_section_shows_the_reason() {
    let state = LoadState::new();
    state.file_name.set("x.apk".to_owned());
    state.input.set(Section::Ready(Arc::new(AppSummary {
        dex_count: 1,
        manifest_error: None,
    })));
    state
        .sdk
        .set(Section::Unavailable("the SDK scan failed: boom".to_owned()));
    let page = mount_view(move || view! { <Results state=state/> });
    wait(50).await;
    assert!(text_of(&page).contains("the SDK scan failed: boom"));
}

#[wasm_bindgen_test]
fn markup_in_a_class_name_stays_text() {
    let classes = Arc::new(ClassRows {
        rows: vec![ClassRow {
            descriptor: "L<b>x</b>;".to_owned(),
            method_count: 0,
            dex_index: 0,
        }],
        unreadable: 0,
    });
    let page = mount_view(move || view! { <ClassesView classes=classes/> });
    assert!(page.query_selector("b").expect("selector").is_none());
    assert!(text_of(&page).contains("L<b>x</b>;"));
}

fn tab_count(page: &HtmlElement) -> u32 {
    page.query_selector_all("[role=tab]")
        .expect("selector")
        .length()
}

#[wasm_bindgen_test]
async fn garbage_apk_shows_an_error_then_sample_loads() {
    let state = LoadState::new();
    let page = mount_view(move || view! { <Results state=state/> });
    state.start_load("junk.apk".to_owned(), b"this is not a zip file".to_vec());
    until(|| !state.busy.get_untracked()).await;
    wait(50).await;
    assert!(
        page.query_selector(".notice.error")
            .expect("selector")
            .is_some()
    );
    assert_eq!(
        tab_count(&page),
        0,
        "tabs on a failed load: {}",
        text_of(&page)
    );
    assert!(!text_of(&page).contains("SDKs"), "{}", text_of(&page));

    state.start_load("sample-app.apk".to_owned(), SAMPLE_APK.to_vec());
    until(|| !state.busy.get_untracked()).await;
    wait(50).await;
    assert!(
        page.query_selector(".notice.error")
            .expect("selector")
            .is_none()
    );
    assert_eq!(tab_count(&page), 4);
    assert!(text_of(&page).contains("Google Mobile Ads"));
}

#[wasm_bindgen_test]
async fn second_load_clears_the_first_result() {
    let state = LoadState::new();
    let page = mount_view(move || view! { <Results state=state/> });
    state.start_load("sample-app.apk".to_owned(), SAMPLE_APK.to_vec());
    until(|| !state.busy.get_untracked()).await;
    wait(50).await;
    assert!(text_of(&page).contains("Google Mobile Ads"));

    state.start_load("second.dex".to_owned(), BARE_DEX.to_vec());
    until(|| !state.busy.get_untracked()).await;
    wait(50).await;
    let text = text_of(&page);
    assert!(!text.contains("Google Mobile Ads"), "stale SDKs: {text}");
    assert!(!text.contains("test fixture"), "stale note: {text}");
    // A bare DEX has no package, so its name is the title and appears once.
    assert_eq!(text.matches("second.dex").count(), 1, "{text}");
}

/// An APK with the sample's manifest and a `classes.dex` that is not a DEX file.
fn apk_with_corrupt_dex() -> Vec<u8> {
    use std::io::{Cursor, Read, Write};
    use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

    let mut sample = ZipArchive::new(Cursor::new(SAMPLE_APK)).expect("the sample is a zip");
    let mut manifest = Vec::new();
    sample
        .by_name("AndroidManifest.xml")
        .expect("the sample has a manifest")
        .read_to_end(&mut manifest)
        .expect("reads");
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("AndroidManifest.xml", options)
        .expect("start");
    zip.write_all(&manifest).expect("write");
    zip.start_file("classes.dex", options).expect("start");
    zip.write_all(b"dex\n035\0 and then nothing that parses")
        .expect("write");
    zip.finish().expect("finish").into_inner()
}

#[wasm_bindgen_test]
async fn corrupt_dex_in_an_apk_keeps_the_manifest() {
    let state = LoadState::new();
    let page = mount_view(move || view! { <Results state=state/> });
    state.start_load("broken.apk".to_owned(), apk_with_corrupt_dex());
    until(|| !state.busy.get_untracked()).await;
    wait(50).await;
    assert_eq!(tab_count(&page), 4, "{}", text_of(&page));
    assert!(text_of(&page).contains("app.fixture"), "{}", text_of(&page));
    assert!(
        matches!(state.findings.get_untracked(), Section::Unavailable(_)),
        "findings: {:?}",
        state.findings.get_untracked()
    );
    let tab_named = |name: &str| -> HtmlElement {
        let tabs = page.query_selector_all("[role=tab]").expect("selector");
        (0..tabs.length())
            .filter_map(|i| tabs.item(i))
            .find(|tab| tab.text_content().unwrap_or_default().starts_with(name))
            .unwrap_or_else(|| panic!("no {name} tab"))
            .unchecked_into()
    };
    tab_named("Findings").click();
    wait(50).await;
    assert!(
        text_of(&page).contains("could not be parsed"),
        "{}",
        text_of(&page)
    );
    tab_named("Manifest").click();
    wait(50).await;
    assert!(text_of(&page).contains("app.fixture"), "{}", text_of(&page));
}
