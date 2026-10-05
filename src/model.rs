use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "info" => Ok(Self::Info),
            "warning" | "warn" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            _ => Err(format!("expected info, warning, or error; got {value:?}")),
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => formatter.write_str("info"),
            Self::Warning => formatter.write_str("warning"),
            Self::Error => formatter.write_str("error"),
        }
    }
}

impl Severity {
    #[must_use]
    pub const fn sarif_level(self) -> &'static str {
        match self {
            Self::Info => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Deadweight,
    Papertrail,
    Vibecheck,
}

impl std::fmt::Display for Module {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deadweight => formatter.write_str("deadweight"),
            Self::Papertrail => formatter.write_str("papertrail"),
            Self::Vibecheck => formatter.write_str("vibecheck"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RuleMetadata {
    pub id: &'static str,
    pub module: Module,
    pub description: &'static str,
    pub default_severity: Severity,
    pub default_confidence: Confidence,
    pub message: &'static str,
    pub suggestion: &'static str,
    pub rationale: &'static str,
    pub examples: &'static [&'static str],
    pub false_positives: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Finding {
    pub path: PathBuf,
    pub location: Location,
    pub rule_id: &'static str,
    pub module: Module,
    pub severity: Severity,
    pub confidence: Confidence,
    pub message: String,
    pub evidence: Option<String>,
    pub observation: Option<String>,
    pub suggestion: &'static str,
}
