// Modified by Jatin Bharati (2026-11-04): rebuilt on the fork's shell, arrival screen and staged load (fork-webapp). See NOTICE.
use leptos::{ev, prelude::*};

use super::{
    AnalysisView,
    arrival::{Arrival, Picker, load_file},
    state::LoadState,
};
use crate::load::{MAX_ROWS, Section, is_tracking_category};

const FORK_URL: &str = "https://github.com/J18Bharati/dex-parser-analyzer";
const ORIGINAL_URL: &str = "https://github.com/dhilipsiva/dex-parser-analyzer";

#[component]
pub fn App() -> impl IntoView {
    let state = LoadState::new();

    // A file dropped outside the drop area must not make the browser open it and leave the page.
    // The first dropped file is the one that is loaded.
    let dragover = window_event_listener(ev::dragover, |ev| ev.prevent_default());
    let drop = window_event_listener(ev::drop, move |ev| {
        ev.prevent_default();
        let file = ev
            .data_transfer()
            .and_then(|transfer| transfer.files())
            .and_then(|files| files.item(0));
        if let Some(file) = file {
            load_file(state, file);
        }
    });
    on_cleanup(move || {
        dragover.remove();
        drop.remove();
    });

    view! {
        <header class="top">
            <h1 class="brand">"Dex Parser Analyzer"</h1>
            <nav class="links" aria-label="Project links">
                <a href=FORK_URL>"fork by J18Bharati"</a>
                <span aria-hidden="true">" · "</span>
                <a href=ORIGINAL_URL>"original by dhilipsiva"</a>
            </nav>
        </header>
        <main>
            {move || {
                if state.file_name.get().is_empty() {
                    view! { <Arrival state=state/> }.into_any()
                } else {
                    view! { <Results state=state/> }.into_any()
                }
            }}
            <DocumentationSection/>
        </main>
        <footer class="foot">
            <span>"Based on dhilipsiva/dex-parser-analyzer. MIT or Apache-2.0."</span>
        </footer>
    }
}

/// The result of a load, shown as plain sections until the tabbed view replaces it.
#[component]
fn Results(state: LoadState) -> impl IntoView {
    let title = move || match state.sdk.get() {
        Section::Ready(report) => report
            .package
            .clone()
            .unwrap_or_else(|| state.file_name.get()),
        _ => state.file_name.get(),
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
        _ => String::new(),
    };
    view! {
        <section class="res">
            <div class="res-head">
                <p class="file-name">{move || state.file_name.get()}</p>
                <Picker state=state/>
            </div>
            {move || state.busy.get().then(|| view! { <p class="notice" role="status">"Reading the file…"</p> })}
            {move || state.error.get().map(|msg| view! { <p class="notice error" role="alert">{msg}</p> })}
            {move || match state.input.get() {
                Section::Ready(input) => input.manifest_error.clone().map(|why| view! {
                    <p class="notice">"The manifest could not be read, so only the code was scanned: " {why}</p>
                }),
                _ => None,
            }}
            <div class="pkg">{title}</div>
            <div class="meta">{meta}</div>
            <SdkSection state=state/>
            <FindingsSection state=state/>
            <ClassesSection state=state/>
        </section>
    }
}

/// A section that is not ready: a note while the load runs, the reason when it failed.
fn waiting<T>(section: &Section<T>, busy: bool) -> Option<AnyView> {
    match section {
        Section::Ready(_) => None,
        Section::Pending => busy.then(|| view! { <p class="muted">"Working…"</p> }.into_any()),
        Section::Unavailable(why) => {
            Some(view! { <p class="notice error">{why.clone()}</p> }.into_any())
        }
    }
}

#[component]
fn SdkSection(state: LoadState) -> impl IntoView {
    view! {
        <section class="panel">
            <h2>"SDKs"</h2>
            {move || {
                let section = state.sdk.get();
                if let Some(note) = waiting(&section, state.busy.get()) {
                    return note;
                }
                let Section::Ready(report) = section else { return ().into_any() };
                if report.matches.is_empty() {
                    return view! { <p class="muted">"No catalogued SDKs found."</p> }.into_any();
                }
                let cards = report.matches.iter().map(|sdk| {
                    let class = if is_tracking_category(&sdk.category) { "sdk warn" } else { "sdk" };
                    let evidence = sdk.evidence.iter().map(|e| {
                        let label = format!("{} under {}", e.count, e.rule);
                        view! {
                            <div class="ev">
                                <span class="src">{e.source.as_str()}</span>
                                <span class="wrap">{label}</span>
                            </div>
                        }
                    }).collect_view();
                    view! {
                        <li class=class>
                            <div class="name">{sdk.name.clone()}</div>
                            <div class="cat">{sdk.category.clone()}</div>
                            {evidence}
                        </li>
                    }
                }).collect_view();
                view! { <ul class="sdks">{cards}</ul> }.into_any()
            }}
        </section>
    }
}

#[component]
fn FindingsSection(state: LoadState) -> impl IntoView {
    view! {
        <section class="panel">
            <h2>"Findings"</h2>
            {move || {
                let section = state.findings.get();
                if let Some(note) = waiting(&section, state.busy.get()) {
                    return note;
                }
                let Section::Ready(merged) = section else { return ().into_any() };
                let capped = merged.capped.then(|| view! {
                    <p class="notice">"A DEX file reached the findings limit, so there may be more."</p>
                });
                let report = std::sync::Arc::new(merged.report.clone());
                view! { {capped} <AnalysisView report=report/> }.into_any()
            }}
        </section>
    }
}

#[component]
fn ClassesSection(state: LoadState) -> impl IntoView {
    view! {
        <section class="panel">
            <h2>"Classes"</h2>
            {move || {
                let section = state.classes.get();
                if let Some(note) = waiting(&section, state.busy.get()) {
                    return note;
                }
                let Section::Ready(classes) = section else { return ().into_any() };
                let shown = classes.rows.len().min(MAX_ROWS);
                let more = (classes.rows.len() > shown).then(|| {
                    format!("Showing the first {shown} of {} classes.", classes.rows.len())
                });
                let skipped = (classes.unreadable > 0).then(|| {
                    format!("{} class definitions could not be read.", classes.unreadable)
                });
                let rows = classes.rows.iter().take(shown).map(|row| {
                    view! {
                        <li class="class-row">
                            <span class="mono wrap">{row.descriptor.clone()}</span>
                            <span class="muted">{row.method_count} " methods"</span>
                        </li>
                    }
                }).collect_view();
                view! {
                    <ul class="classes">{rows}</ul>
                    {more.map(|text| view! { <p class="muted">{text}</p> })}
                    {skipped.map(|text| view! { <p class="muted">{text}</p> })}
                }.into_any()
            }}
        </section>
    }
}

const DEX_CORE_SNIPPET: &str = r#"use dexpa_core::{parse_dex, DexError};

fn print_classes(bytes: &[u8]) -> Result<(), DexError> {
    let dex = parse_dex(bytes)?;
    println!("DEX version {}", dex.header().version);
    for class in dex.classes() {
        let descriptor = class.descriptor()?;
        println!("class: {descriptor}");
    }
    Ok(())
}"#;

const DEX_ANALYSIS_SNIPPET: &str = r#"use dexpa_analysis::{config::AnalysisConfig, engine::analyze_dex};

fn run_analysis(bytes: &[u8]) -> anyhow::Result<()> {
    let dex = parse_dex(bytes)?;
    let report = analyze_dex(&dex, &AnalysisConfig::default());
    for finding in report.findings {
        println!("{:?}: {}", finding.severity, finding.message);
    }
    Ok(())
}"#;

const DEX_CLI_SNIPPET: &str = r#"cargo run -p dexpa-cli -- path/to/classes.dex --max-findings 25"#;

#[component]
fn DocumentationSection() -> impl IntoView {
    view! {
        <section class="docs">
            <h2>"Sample Usage"</h2>
            <div class="snippets">
                <div>
                    <h3>"dex-core"</h3>
                    <pre class="code"><code class="language-rust">{DEX_CORE_SNIPPET}</code></pre>
                </div>
                <div>
                    <h3>"dex-analysis"</h3>
                    <pre class="code"><code class="language-rust">{DEX_ANALYSIS_SNIPPET}</code></pre>
                </div>
                <div>
                    <h3>"dexpa-cli"</h3>
                    <pre class="code"><code class="language-bash">{DEX_CLI_SNIPPET}</code></pre>
                </div>
            </div>
        </section>
    }
}
