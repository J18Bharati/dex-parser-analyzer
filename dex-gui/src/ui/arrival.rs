// Modified by Jatin Bharati (2026-11-04): new module, the arrival screen and the file picker (fork-webapp). See NOTICE.
use gloo_file::{File as GlooFile, futures::read_as_bytes};
use leptos::web_sys::{File, HtmlInputElement};
use leptos::{prelude::*, task::spawn_local};

use super::state::LoadState;
use crate::load::{SAMPLE_APK, check_size};

pub const SAMPLE_NAME: &str = "sample-app.apk";

/// Loads `file`, refusing one over the size limit before reading it. Ignored while busy.
pub fn load_file(state: LoadState, file: File) {
    if state.busy.get_untracked() {
        return;
    }
    let file = GlooFile::from(file);
    let name = file.name();
    if let Err(message) = check_size(file.size()) {
        state.fail(name, message);
        return;
    }
    // Hold the busy state while the file is read, so a second pick cannot overlap this one.
    state.busy.set(true);
    spawn_local(async move {
        let read = read_as_bytes(&file).await;
        state.busy.set(false);
        match read {
            Ok(bytes) => state.start_load(name, bytes),
            Err(err) => state.fail(name, format!("failed to read the file: {err}")),
        }
    });
}

/// The `choose file` and `try a sample app` controls.
#[component]
pub fn Picker(state: LoadState) -> impl IntoView {
    let on_change = move |ev: leptos::ev::Event| {
        let input: HtmlInputElement = event_target(&ev);
        let file = input.files().and_then(|files| files.item(0));
        // Clear the value, so choosing the same file again fires `change` again.
        input.set_value("");
        if let Some(file) = file {
            load_file(state, file);
        }
    };
    let on_sample = move |_| state.start_load(SAMPLE_NAME.to_owned(), SAMPLE_APK.to_vec());
    view! {
        <div class="actions">
            <label class="btn" class:disabled=move || state.busy.get()>
                "choose file"
                <input
                    class="file-input"
                    type="file"
                    accept=".apk,.dex"
                    disabled=move || state.busy.get()
                    on:change=on_change
                />
            </label>
            <button
                class="btn ghost"
                type="button"
                disabled=move || state.busy.get()
                on:click=on_sample
            >
                "try a sample app"
            </button>
        </div>
    }
}

/// The first screen: what the page does, where to drop a file, and what the fork adds.
#[component]
pub fn Arrival(state: LoadState) -> impl IntoView {
    view! {
        <section class="hero">
            <h2>"See what's inside an Android app"</h2>
            <p class="lede">
                "Find the third-party SDKs an app ships, and scan its code for security issues."
            </p>
            <div class="drop">
                <strong>"Drop an APK or DEX file here"</strong>
                <span class="privacy">"The file is read in this page and never uploaded."</span>
                <Picker state=state/>
            </div>
        </section>
        <ul class="features">
            <li class="feature">
                <strong>"SDK detection"</strong>
                <em>"this fork"</em>
                <p>"Which SDKs an app contains, with the evidence for each."</p>
            </li>
            <li class="feature">
                <strong>"Manifest"</strong>
                <em>"this fork"</em>
                <p>"Package, version, permissions and components from the APK."</p>
            </li>
            <li class="feature">
                <strong>"Code analysis"</strong>
                <p>"Static-analysis findings and a class browser, from the original project."</p>
            </li>
        </ul>
    }
}
