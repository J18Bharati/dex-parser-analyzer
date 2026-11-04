// Modified by Jatin Bharati (2026-11-04): classes from the fork's stylesheet, rows capped at MAX_ROWS, keyboard-reachable rows, labelled finding details, a cap note (fork-webapp). See NOTICE.
use std::sync::Arc;

use dexpa_analysis::model::{AnalysisReport, Finding, Severity};
use leptos::prelude::*;

use crate::load::MAX_ROWS;

const SEVERITY_ORDER: [Severity; 5] = [
    Severity::Critical,
    Severity::High,
    Severity::Medium,
    Severity::Low,
    Severity::Info,
];

#[component]
pub fn AnalysisView(
    report: Arc<AnalysisReport>,
    /// A DEX file reached the findings limit, so the list may be incomplete.
    #[prop(optional)]
    capped: bool,
) -> impl IntoView {
    let (active_severities, set_active_severities) = signal(SEVERITY_ORDER.to_vec());
    let (selected, set_selected) = signal(Option::<Finding>::None);
    let findings_source = Arc::clone(&report);

    let count_source = Arc::clone(&report);
    let filtered_count = move || {
        let active = active_severities.get();
        count_source
            .findings
            .iter()
            .filter(|finding| active.contains(&finding.severity))
            .count()
    };
    let filtered_findings = move || {
        let active = active_severities.get();
        findings_source
            .findings
            .iter()
            .filter(|finding| active.contains(&finding.severity))
            .take(MAX_ROWS)
            .cloned()
            .collect::<Vec<_>>()
    };

    let toggle = move |severity: Severity| {
        set_active_severities.update(|current| {
            if current.contains(&severity) {
                current.retain(|s| *s != severity);
            } else {
                current.push(severity);
            }
        });
    };

    let stat = |label: &'static str, value: String| {
        view! {
            <div class="stat">
                <div class="stat-label">{label}</div>
                <div class="stat-value">{value}</div>
            </div>
        }
    };
    let elapsed = report
        .stats
        .elapsed_ms
        .map(|elapsed| stat("Elapsed", format!("{elapsed} ms")));
    let total = report.findings.len();

    view! {
        <div class="stack">
            {capped.then(|| view! {
                <p class="notice">"Capped at 500 findings per DEX file, so the real total may be higher."</p>
            })}
            <section>
                <div class="stats">
                    {stat("Findings", total.to_string())}
                    {stat("Methods", report.stats.method_count.to_string())}
                    {stat("Strings", report.stats.string_count.to_string())}
                    {stat("Instructions", report.stats.instruction_count.to_string())}
                    {elapsed}
                </div>
                <div class="chips">
                    { SEVERITY_ORDER.iter().map(|severity| {
                        let label = format!("{severity:?}");
                        let sev = *severity;
                        view! {
                            <button
                                type="button"
                                class="chip"
                                class:on=move || active_severities.get().contains(&sev)
                                aria-pressed=move || active_severities.get().contains(&sev).to_string()
                                on:click=move |_| toggle(sev)
                            >
                                {label.clone()}
                            </button>
                        }
                    }).collect_view() }
                </div>
            </section>

            <section>
                <div class="table-wrap">
                    <table class="table">
                        <thead>
                            <tr>
                                <th scope="col">"Severity"</th>
                                <th scope="col">"Rule"</th>
                                <th scope="col">"Location"</th>
                                <th scope="col">"Message"</th>
                            </tr>
                        </thead>
                        <tbody>
                            { move || {
                                let rows = filtered_findings();
                                if rows.is_empty() {
                                    return view! {
                                        <tr>
                                            <td colspan="4" class="muted">
                                                "No findings with the selected severities."
                                            </td>
                                        </tr>
                                    }.into_any();
                                }
                                rows.into_iter().map(|finding| {
                                    let severity = format!("{:?}", finding.severity);
                                    let rule = finding.id.to_string();
                                    let location = format!(
                                        "{}::{}",
                                        finding.location.class_descriptor, finding.location.method_name
                                    );
                                    let message = finding.message.clone();
                                    let row_finding = finding.clone();
                                    let button_finding = finding.clone();
                                    view! {
                                        <tr on:click=move |_| set_selected.set(Some(row_finding.clone())) class="clickable">
                                            <td>{severity}</td>
                                            <td class="nowrap">{rule}</td>
                                            <td class="mono wrap">
                                                <button
                                                    type="button"
                                                    class="linkish"
                                                    on:click=move |_| set_selected.set(Some(button_finding.clone()))
                                                >
                                                    {location}
                                                </button>
                                            </td>
                                            <td class="wrap">{message}</td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }}
                        </tbody>
                    </table>
                </div>
                { move || (filtered_count() > MAX_ROWS).then(|| view! {
                    <p class="muted">{format!("showing {MAX_ROWS} of {} findings", filtered_count())}</p>
                }) }
            </section>

            { move || selected.get().map(|finding| {
                view! {
                    <section>
                        <h3>"Finding details"</h3>
                        <FindingDetails finding=finding/>
                    </section>
                }
            }) }
        </div>
    }
}

/// The fields of one finding, labelled.
#[component]
fn FindingDetails(finding: Finding) -> impl IntoView {
    let mut fields = vec![
        ("Rule", finding.id.to_string()),
        ("Severity", format!("{:?}", finding.severity)),
        ("Kind", format!("{:?}", finding.kind)),
        ("Class", finding.location.class_descriptor.clone()),
        ("Method", finding.location.method_name.clone()),
        ("Signature", finding.location.method_signature.clone()),
    ];
    if let Some(pc) = finding.location.pc {
        fields.push(("Offset", pc.to_string()));
    }
    if let Some(cwe) = finding.cwe {
        fields.push(("CWE", format!("CWE-{cwe}")));
    }
    fields.push(("Message", finding.message.clone()));
    if let Some(remediation) = &finding.remediation {
        fields.push(("Remediation", remediation.clone()));
    }
    if !finding.extra.is_null() {
        fields.push(("Extra", finding.extra.to_string()));
    }
    let rows = fields
        .into_iter()
        .map(|(label, value)| {
            view! {
                <dt>{label}</dt>
                <dd class="mono wrap">{value}</dd>
            }
        })
        .collect_view();
    view! { <dl class="kv">{rows}</dl> }
}
