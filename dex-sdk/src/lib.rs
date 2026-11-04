// Modified by Jatin Bharati (2026-11-01): new crate (fork M2); scan module (2026-11-04). See NOTICE.
#![deny(unsafe_code)]
#![deny(unused_must_use)]

//! `dexpa-sdk`: which code an APK defines, and which SDKs that code and the manifest point to.
//!
//! [`inventory`] counts the classes an app *defines*, grouped by package prefix. Classes that are
//! only referenced (`java.lang.Object`, a Play Services API stub the app calls) are not counted.

pub mod catalogue;
pub mod diff;
pub mod inventory;
pub mod matcher;
pub mod report;
pub mod scan;
