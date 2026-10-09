#![forbid(unsafe_code)]

pub mod analysis;
pub mod attributes;
pub mod benchmark;
pub mod config;
pub mod discovery;
pub mod git;
pub mod init;
pub mod language;
#[cfg(feature = "lsp")]
pub mod lsp;
pub mod model;
pub mod papertrail;
#[cfg(feature = "polygraph")]
pub mod polygraph;
pub mod reporting;
pub mod rules;
pub mod scanner;
pub mod suppression;

pub use model::{Confidence, Finding, Module, RuleMetadata, Severity};
pub use scanner::{ScanOptions, ScanResult, SourceFile, scan_paths, scan_sources};
