// Modified by Jatin Bharati (2026-11-04): rebuilt on the fork's shell, arrival screen and staged load; the results moved to `results` (fork-webapp). See NOTICE.
use leptos::{ev, prelude::*};

use super::{
    arrival::{Arrival, load_file},
    results::Results,
    state::LoadState,
};

const FORK_URL: &str = "https://github.com/J18Bharati/dex-parser-analyzer";
const ADDS_URL: &str = "https://github.com/J18Bharati/dex-parser-analyzer#what-this-fork-adds";
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
            {move || state.busy.get().then(|| view! { <p class="notice busy" role="status">"Working…"</p> })}
            {move || {
                if state.file_name.get().is_empty() {
                    view! { <Arrival state=state/> }.into_any()
                } else {
                    view! { <Results state=state/> }.into_any()
                }
            }}
            {move || state.file_name.get().is_empty().then(|| view! { <DocumentationSection/> })}
        </main>
        <footer class="foot">
            <span>"Based on dhilipsiva/dex-parser-analyzer. MIT or Apache-2.0."</span>
            <span aria-hidden="true">" · "</span>
            <a href=ADDS_URL>"what this fork adds"</a>
        </footer>
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
