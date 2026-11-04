// Modified by Jatin Bharati (2026-11-04): new module, the tabbed results (fork-webapp). See NOTICE.
use std::sync::Arc;

use dexpa_sdk::report::value_text;
use leptos::prelude::*;

use super::{
    AnalysisView, dex_view::ClassesView, manifest_view::ManifestView, sdk_view::SdkView,
    state::LoadState,
};
use crate::load::Section;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Sdks,
    Manifest,
    Findings,
    Classes,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Sdks, "SDKs"),
    (Tab::Manifest, "Manifest"),
    (Tab::Findings, "Findings"),
    (Tab::Classes, "Classes"),
];

/// The result of a load: the error if it failed, else the title and the tabs once the input is
/// read.
#[component]
pub fn Results(state: LoadState) -> impl IntoView {
    let selected = RwSignal::new(Tab::Sdks);
    // A new load starts on the SDK tab.
    Effect::new(move |_| {
        state.file_name.track();
        selected.set(Tab::Sdks);
    });
    let ready = move || matches!(state.input.get(), Section::Ready(_));
    // A bare DEX has no package, so the file name is the title and is not shown twice.
    let name_is_title = move || ready() && state.manifest.get().is_none_or(|m| m.package.is_none());
    view! {
        <section class="res">
            <div class="res-head">
                <p class="file-name">{move || (!name_is_title()).then(|| state.file_name.get())}</p>
                <button
                    type="button"
                    class="btn ghost load-another"
                    disabled=move || state.busy.get()
                    on:click=move |_| state.clear()
                >
                    "load another file"
                </button>
            </div>
            {move || state.error.get().map(|msg| view! { <p class="notice error" role="alert">{msg}</p> })}
            {move || ready().then(|| view! { <Loaded state=state selected=selected/> })}
        </section>
    }
}

/// Everything shown once the file has been read.
#[component]
fn Loaded(state: LoadState, selected: RwSignal<Tab>) -> impl IntoView {
    // The package, else the file name.
    let title = move || {
        state
            .manifest
            .get()
            .and_then(|manifest| manifest.package.clone())
            .unwrap_or_else(|| state.file_name.get())
    };
    let meta = move || match state.sdk.get() {
        Section::Ready(report) => {
            let version = match (&report.version_name, &report.version_code) {
                (Some(name), Some(code)) => format!("v{name} ({code}) · "),
                (Some(name), None) => format!("v{name} · "),
                (None, Some(code)) => format!("version code {code} · "),
                (None, None) => String::new(),
            };
            format!(
                "{version}{} DEX {} · {} classes · {} of {} catalogued SDKs found",
                report.dex_count,
                if report.dex_count == 1 {
                    "file"
                } else {
                    "files"
                },
                report.class_count,
                report.matches.len(),
                report.catalogue_size,
            )
        }
        // The SDK section failed: keep what the manifest and the file summary still say.
        _ => {
            let manifest = state.manifest.get();
            let version = manifest.as_deref().map_or_else(String::new, |m| {
                let name = m.version_name.as_ref().and_then(value_text);
                let code = m.version_code.as_ref().and_then(value_text);
                match (name, code) {
                    (Some(name), Some(code)) => format!("v{name} ({code})"),
                    (Some(name), None) => format!("v{name}"),
                    (None, Some(code)) => format!("version code {code}"),
                    (None, None) => String::new(),
                }
            });
            let files = match state.input.get() {
                Section::Ready(input) => Some(format!(
                    "{} DEX {}",
                    input.dex_count,
                    if input.dex_count == 1 {
                        "file"
                    } else {
                        "files"
                    }
                )),
                _ => None,
            };
            [(!version.is_empty()).then_some(version), files]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
        }
    };
    let notices = move || {
        let mut notes = Vec::new();
        if let Section::Ready(input) = state.input.get()
            && let Some(why) = &input.manifest_error
        {
            notes.push(format!(
                "The manifest could not be read, so only the code was scanned: {why}"
            ));
        }
        if let Section::Ready(report) = state.sdk.get()
            && report.unreadable_classes > 0
        {
            notes.push(format!(
                "{} class definitions could not be read, so the SDK scan may miss some.",
                report.unreadable_classes
            ));
        }
        if let Section::Ready(input) = state.input.get()
            && input.manifest_error.is_none()
            && state.manifest.get().is_none()
            && matches!(state.sdk.get(), Section::Ready(_))
        {
            notes.push("No manifest was read, so only classes were checked for SDKs.".to_owned());
        }
        if state.is_sample.get() {
            notes.push("This is a test fixture with stub classes, not a real app.".to_owned());
        }
        notes
            .into_iter()
            .map(|text| view! { <p class="notice">{text}</p> })
            .collect_view()
    };
    let count = move |tab: Tab| -> Option<usize> {
        match tab {
            Tab::Sdks => match state.sdk.get() {
                Section::Ready(report) => Some(report.matches.len()),
                _ => None,
            },
            Tab::Findings => match state.findings.get() {
                Section::Ready(merged) => Some(merged.report.findings.len()),
                _ => None,
            },
            Tab::Classes => match state.classes.get() {
                Section::Ready(classes) => Some(classes.rows.len()),
                _ => None,
            },
            Tab::Manifest => None,
        }
    };
    let buttons = TABS
        .iter()
        .map(|&(tab, label)| {
            view! {
                <button
                    type="button"
                    role="tab"
                    class="tab"
                    class:on=move || selected.get() == tab
                    aria-selected=move || (selected.get() == tab).to_string()
                    on:click=move |_| selected.set(tab)
                >
                    {label}
                    {move || count(tab).map(|n| view! { " " <span class="count">{n}</span> })}
                </button>
            }
        })
        .collect_view();
    view! {
        <div class="pkg">{title}</div>
        <div class="meta">{meta}</div>
        {notices}
        <div class="tabs" role="tablist">{buttons}</div>
        <div class="tabpanel" role="tabpanel">
            {move || match selected.get() {
                Tab::Sdks => view! { <SdkTab state=state/> }.into_any(),
                Tab::Manifest => view! { <ManifestTab state=state/> }.into_any(),
                Tab::Findings => view! { <FindingsTab state=state/> }.into_any(),
                Tab::Classes => view! { <ClassesTab state=state/> }.into_any(),
            }}
        </div>
    }
}

/// A section that is not ready: a note while the load runs, the reason when it failed.
fn waiting<T>(section: &Section<T>, busy: bool) -> Option<AnyView> {
    match section {
        Section::Ready(_) => None,
        Section::Pending => {
            busy.then(|| view! { <p class="muted">"still analysing"</p> }.into_any())
        }
        Section::Unavailable(why) => {
            Some(view! { <p class="notice error">{why.clone()}</p> }.into_any())
        }
    }
}

#[component]
fn SdkTab(state: LoadState) -> impl IntoView {
    move || {
        let section = state.sdk.get();
        if let Some(note) = waiting(&section, state.busy.get()) {
            return note;
        }
        let Section::Ready(report) = section else {
            return ().into_any();
        };
        view! { <SdkView report=report/> }.into_any()
    }
}

#[component]
fn ManifestTab(state: LoadState) -> impl IntoView {
    move || {
        let error = match state.input.get() {
            Section::Ready(input) => input.manifest_error.clone(),
            _ => None,
        };
        let is_apk = match state.input.get() {
            Section::Ready(input) => input.is_apk,
            _ => false,
        };
        view! { <ManifestView manifest=state.manifest.get() error=error is_apk=is_apk/> }
    }
}

#[component]
fn FindingsTab(state: LoadState) -> impl IntoView {
    move || {
        let section = state.findings.get();
        if let Some(note) = waiting(&section, state.busy.get()) {
            return note;
        }
        let Section::Ready(merged) = section else {
            return ().into_any();
        };
        let report = Arc::new(merged.report.clone());
        view! { <AnalysisView report=report capped=merged.capped/> }.into_any()
    }
}

#[component]
fn ClassesTab(state: LoadState) -> impl IntoView {
    move || {
        let section = state.classes.get();
        if let Some(note) = waiting(&section, state.busy.get()) {
            return note;
        }
        let Section::Ready(classes) = section else {
            return ().into_any();
        };
        view! { <ClassesView classes=classes/> }.into_any()
    }
}
