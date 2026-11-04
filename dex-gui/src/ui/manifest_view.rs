// Modified by Jatin Bharati (2026-11-04): new module, the manifest tab (fork-webapp). See NOTICE.
use std::sync::Arc;

use dexpa_core::manifest::{Component, ComponentKind, Manifest};
use dexpa_sdk::report::value_text;
use leptos::prelude::*;

use crate::load::MAX_ROWS;

fn kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Activity => "activity",
        ComponentKind::ActivityAlias => "activity-alias",
        ComponentKind::Service => "service",
        ComponentKind::Receiver => "receiver",
        ComponentKind::Provider => "provider",
    }
}

fn component_text(component: &Component, package: Option<&str>) -> String {
    let name = component
        .qualified_name(package)
        .or_else(|| component.name.clone())
        .unwrap_or_else(|| "(unnamed)".to_owned());
    match component.exported {
        Some(true) => format!("{name} (exported)"),
        _ => name,
    }
}

/// A heading and a list of `(label, text)` rows, at most `MAX_ROWS` of them.
fn list_block(title: &str, rows: Vec<(String, String)>) -> AnyView {
    if rows.is_empty() {
        return ().into_any();
    }
    let total = rows.len();
    let note = (total > MAX_ROWS).then(|| format!("showing {MAX_ROWS} of {total}"));
    let items = rows
        .into_iter()
        .take(MAX_ROWS)
        .map(|(label, text)| {
            view! {
                <li class="class-row">
                    <span class="mono wrap">{text}</span>
                    <span class="muted wrap">{label}</span>
                </li>
            }
        })
        .collect_view();
    view! {
        <h3>{format!("{title} ({total})")}</h3>
        <ul class="classes">{items}</ul>
        {note.map(|text| view! { <p class="muted">{text}</p> })}
    }
    .into_any()
}

/// The manifest of an APK, or the reason there is none.
#[component]
pub fn ManifestView(
    manifest: Option<Arc<Manifest>>,
    error: Option<String>,
    /// The input is an APK; a bare DEX file has no manifest to read.
    #[prop(optional)]
    is_apk: bool,
) -> impl IntoView {
    let Some(manifest) = manifest else {
        return match error {
            Some(why) => view! {
                <p class="notice">"The manifest could not be read: " {why}</p>
            }
            .into_any(),
            None if is_apk => view! {
                <p class="muted">"No manifest: this APK has no AndroidManifest.xml."</p>
            }
            .into_any(),
            None => view! {
                <p class="muted">"No manifest: this was a DEX file, not an APK."</p>
            }
            .into_any(),
        };
    };
    let text = |value: &Option<dexpa_core::axml::XmlValue>| value.as_ref().and_then(value_text);
    let fields = [
        ("package", manifest.package.clone()),
        ("version name", text(&manifest.version_name)),
        ("version code", text(&manifest.version_code)),
        ("min SDK", text(&manifest.min_sdk)),
        ("target SDK", text(&manifest.target_sdk)),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        value.map(|value| {
            view! {
                <dt>{label}</dt>
                <dd class="mono wrap">{value}</dd>
            }
        })
    })
    .collect_view();
    let permissions = manifest
        .uses_permissions
        .iter()
        .map(|name| (String::new(), name.clone()))
        .collect();
    let package = manifest.package.as_deref();
    let components = manifest
        .components
        .iter()
        .map(|c| (kind_label(c.kind).to_owned(), component_text(c, package)))
        .collect();
    let meta = manifest
        .meta_data
        .iter()
        .map(|m| {
            let name = m.name.clone().unwrap_or_else(|| "(unnamed)".to_owned());
            let value = m.value.as_ref().and_then(value_text).unwrap_or_default();
            (
                m.owner.clone().unwrap_or_default(),
                format!("{name} = {value}"),
            )
        })
        .collect();
    view! {
        <dl class="kv">{fields}</dl>
        {list_block("Permissions", permissions)}
        {list_block("Components", components)}
        {list_block("Meta-data", meta)}
    }
    .into_any()
}
