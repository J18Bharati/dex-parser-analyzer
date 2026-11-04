// Modified by Jatin Bharati (2026-11-04): arrival and state modules; `DexView` is unused until the tabbed results replace it (fork-webapp). See NOTICE.
mod analysis_view;
mod app;
mod arrival;
// Still on Bootstrap classes; the tabbed class browser replaces it.
#[allow(dead_code)]
mod dex_view;
mod state;

pub use analysis_view::AnalysisView;
pub use app::App;
