// Modified by Jatin Bharati (2026-11-04): new module, the SDK cards (fork-webapp). See NOTICE.
use std::sync::Arc;

use dexpa_sdk::{
    matcher::{Evidence, Source},
    report::SdkReport,
};
use leptos::prelude::*;

use crate::load::is_tracking_category;

/// The label of an evidence source.
fn source_label(source: Source) -> &'static str {
    match source {
        Source::DexClass => "classes",
        Source::ManifestComponent => "manifest",
        Source::ManifestMeta => "meta-data",
    }
}

/// What one piece of evidence matched: a meta-data key is the rule itself, the rest count items.
fn evidence_text(evidence: &Evidence) -> String {
    match evidence.source {
        Source::ManifestMeta => evidence.rule.clone(),
        _ => format!("{} under {}", evidence.count, evidence.rule),
    }
}

/// The SDKs found, one card each with its evidence, and the caveat about what is not found.
#[component]
pub fn SdkView(report: Arc<SdkReport>) -> impl IntoView {
    let caveat = format!(
        "Not finding an SDK doesn't prove it's absent: only {} SDKs are checked, and build tools can rename classes.",
        report.catalogue_size
    );
    let list = if report.matches.is_empty() {
        view! { <p class="muted">"No catalogued SDKs found."</p> }.into_any()
    } else {
        let cards = report
            .matches
            .iter()
            .map(|sdk| {
                let class = if is_tracking_category(&sdk.category) {
                    "sdk warn"
                } else {
                    "sdk"
                };
                let evidence = sdk
                    .evidence
                    .iter()
                    .map(|e| {
                        view! {
                            <div class="ev">
                                <span class="src">{source_label(e.source)}</span>
                                <span class="wrap">{evidence_text(e)}</span>
                            </div>
                        }
                    })
                    .collect_view();
                view! {
                    <li class=class>
                        <div class="name wrap">{sdk.name.clone()}</div>
                        <div class="cat">{sdk.category.clone()}</div>
                        {evidence}
                    </li>
                }
            })
            .collect_view();
        view! { <ul class="sdks">{cards}</ul> }.into_any()
    };
    view! {
        {list}
        <p class="muted caveat">{caveat}</p>
    }
}
