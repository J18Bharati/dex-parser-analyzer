// Modified by Jatin Bharati (2026-11-04): `ClassesView` replaces the Bootstrap `DexView`; rows capped at MAX_ROWS (fork-webapp). See NOTICE.
use std::sync::Arc;

use leptos::prelude::*;

use crate::load::{ClassRows, MAX_ROWS};

/// The classes of the file, with a filter on the descriptor that ignores case.
#[component]
pub fn ClassesView(classes: Arc<ClassRows>) -> impl IntoView {
    let (filter, set_filter) = signal(String::new());
    let source = Arc::clone(&classes);
    let unreadable = (classes.unreadable > 0).then(|| {
        format!(
            "{} class definitions could not be read.",
            classes.unreadable
        )
    });
    view! {
        <input
            class="filter"
            type="text"
            placeholder="Filter by class name…"
            aria-label="Filter classes"
            prop:value=move || filter.get()
            on:input=move |ev| set_filter.set(event_target_value(&ev))
        />
        {move || {
            let needle = filter.get().to_lowercase();
            let rows = source
                .rows
                .iter()
                .filter(|row| needle.is_empty() || row.descriptor.to_lowercase().contains(&needle))
                .collect::<Vec<_>>();
            if rows.is_empty() {
                return view! { <p class="muted">"No classes match the filter."</p> }.into_any();
            }
            let total = rows.len();
            let note = (total > MAX_ROWS).then(|| format!("showing {MAX_ROWS} of {total} classes"));
            let items = rows
                .into_iter()
                .take(MAX_ROWS)
                .map(|row| {
                    view! {
                        <li class="class-row">
                            <span class="mono wrap">{row.descriptor.clone()}</span>
                            <span class="muted">{row.method_count} " methods"</span>
                        </li>
                    }
                })
                .collect_view();
            view! {
                <ul class="classes">{items}</ul>
                {note.map(|text| view! { <p class="muted">{text}</p> })}
            }
            .into_any()
        }}
        {unreadable.map(|text| view! { <p class="muted">{text}</p> })}
    }
}
