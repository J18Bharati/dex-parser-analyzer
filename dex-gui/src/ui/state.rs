// Modified by Jatin Bharati (2026-11-04): new module, the staged load (fork-webapp). See NOTICE.
//! The page's load state: one signal per part of a result, filled in stages so the page can show
//! what is ready while the rest is still being computed.

use std::sync::Arc;

use dexpa_analysis::{config::AnalysisConfig, engine::analyze_dex};
use dexpa_core::{manifest::Manifest, parse_dex};
use dexpa_sdk::{
    catalogue,
    report::SdkReport,
    scan::{read_input, report_for},
};
use js_sys::Promise;
use leptos::{prelude::*, task::spawn_local};
use wasm_bindgen_futures::JsFuture;

use crate::load::{
    ClassRows, GUI_LIMITS, MergedFindings, Section, check_size, class_rows, merge_reports,
    scan_error_text,
};

/// What was read from the file, before any analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSummary {
    /// DEX files in the file.
    pub dex_count: usize,
    /// Why a present manifest could not be read.
    pub manifest_error: Option<String>,
}

/// The signals of one page. `Copy`, so event handlers take it by value.
#[derive(Clone, Copy)]
pub struct LoadState {
    /// A load is running; the controls are disabled meanwhile.
    pub busy: RwSignal<bool>,
    /// Why the last load failed.
    pub error: RwSignal<Option<String>>,
    /// The file being or last loaded; empty before the first.
    pub file_name: RwSignal<String>,
    /// What was read from the file.
    pub input: RwSignal<Section<Arc<AppSummary>>>,
    /// The SDK report.
    pub sdk: RwSignal<Section<Arc<SdkReport>>>,
    /// The manifest, for an APK that has a readable one.
    pub manifest: RwSignal<Option<Arc<Manifest>>>,
    /// The static-analysis findings of every DEX file.
    pub findings: RwSignal<Section<Arc<MergedFindings>>>,
    /// The classes of every DEX file.
    pub classes: RwSignal<Section<Arc<ClassRows>>>,
}

/// Waits `ms` milliseconds, which lets the browser paint and handle events.
async fn pause(ms: i32) {
    let promise = Promise::new(&mut |resolve, _reject| {
        let scheduled = leptos::web_sys::window().is_some_and(|window| {
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
                .is_ok()
        });
        if !scheduled {
            let _ = resolve.call0(&leptos::wasm_bindgen::JsValue::UNDEFINED);
        }
    });
    let _ = JsFuture::from(promise).await;
}

impl LoadState {
    /// A state with nothing loaded.
    pub fn new() -> Self {
        Self {
            busy: RwSignal::new(false),
            error: RwSignal::new(None),
            file_name: RwSignal::new(String::new()),
            input: RwSignal::new(Section::Pending),
            sdk: RwSignal::new(Section::Pending),
            manifest: RwSignal::new(None),
            findings: RwSignal::new(Section::Pending),
            classes: RwSignal::new(Section::Pending),
        }
    }

    /// Forgets the previous result, so nothing stale remains.
    fn reset(self, name: String) {
        self.error.set(None);
        self.file_name.set(name);
        self.input.set(Section::Pending);
        self.sdk.set(Section::Pending);
        self.manifest.set(None);
        self.findings.set(Section::Pending);
        self.classes.set(Section::Pending);
    }

    /// Shows `message` as the result of loading `name`, without loading anything.
    pub fn fail(self, name: String, message: String) {
        if self.busy.get_untracked() {
            return;
        }
        self.reset(name);
        self.error.set(Some(message));
    }

    /// Loads `bytes`, a `.dex` or `.apk` file called `name`, in stages: the SDK report first,
    /// then the findings and the class list one DEX file at a time. Ignored while a load runs.
    pub fn start_load(self, name: String, bytes: Vec<u8>) {
        if self.busy.get_untracked() {
            return;
        }
        self.reset(name);
        self.busy.set(true);
        spawn_local(async move {
            self.run(bytes).await;
            self.busy.set(false);
        });
    }

    async fn run(self, bytes: Vec<u8>) {
        // Let the browser show the busy state before the work blocks it.
        pause(30).await;
        if let Err(message) = check_size(bytes.len() as u64) {
            self.error.set(Some(message));
            return;
        }
        let input = match read_input(&bytes, GUI_LIMITS) {
            Ok(input) => input,
            Err(err) => {
                self.error.set(Some(scan_error_text(&err)));
                return;
            }
        };
        drop(bytes);
        self.input.set(Section::Ready(Arc::new(AppSummary {
            dex_count: input.buffers.len(),
            manifest_error: input.manifest_error.clone(),
        })));
        self.manifest.set(input.manifest.clone().map(Arc::new));

        pause(0).await;
        match report_for(&catalogue::bundled(), &input) {
            Ok(report) => self.sdk.set(Section::Ready(Arc::new(report))),
            Err(err) => self.sdk.set(Section::Unavailable(err.to_string())),
        }

        let config = AnalysisConfig::default();
        let mut reports = Vec::with_capacity(input.buffers.len());
        let mut rows = Vec::new();
        let mut unreadable = 0;
        for (index, buffer) in input.buffers.iter().enumerate() {
            pause(0).await;
            let dex = match parse_dex(buffer) {
                Ok(dex) => dex,
                Err(err) => {
                    let why = format!("DEX file {index} could not be parsed: {err}");
                    self.findings.set(Section::Unavailable(why.clone()));
                    self.classes.set(Section::Unavailable(why));
                    return;
                }
            };
            reports.push(analyze_dex(&dex, &config));
            let part = class_rows(&dex, index);
            unreadable += part.unreadable;
            rows.extend(part.rows);
        }
        self.findings.set(Section::Ready(Arc::new(merge_reports(
            reports,
            config.max_findings,
        ))));
        self.classes
            .set(Section::Ready(Arc::new(ClassRows { rows, unreadable })));
    }
}

impl Default for LoadState {
    fn default() -> Self {
        Self::new()
    }
}
