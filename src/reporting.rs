use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Serialize;
use serde_json::json;

use crate::model::{Finding, Severity};
use crate::rules::metadata_registry;
use crate::scanner::ScanResult;

mod html;

pub use html::{HtmlContext, write_html};

/// Writes the human-readable report.
///
/// # Errors
///
/// Returns the underlying writer error.
pub fn write_text(
    mut writer: impl Write,
    result: &ScanResult,
    quiet: bool,
    color: bool,
) -> io::Result<()> {
    if !quiet {
        writeln!(writer, "slopcop {}", env!("CARGO_PKG_VERSION"))?;
    }

    for finding in &result.findings {
        if color {
            let code = match finding.severity {
                Severity::Info => "36",
                Severity::Warning => "33",
                Severity::Error => "31",
            };
            writeln!(
                writer,
                "{}:{}:{}  \x1b[{code}m{}  {}\x1b[0m",
                finding.path.display(),
                finding.location.line,
                finding.location.column,
                finding.rule_id,
                finding.severity,
            )?;
        } else {
            writeln!(
                writer,
                "{}:{}:{}  {}  {}",
                finding.path.display(),
                finding.location.line,
                finding.location.column,
                finding.rule_id,
                finding.severity,
            )?;
        }
        writeln!(writer, "{}", finding.message)?;
        if let Some(observation) = &finding.observation {
            writeln!(writer, "observed: {observation}")?;
        }
    }

    if !quiet {
        if result.findings.is_empty() {
            writeln!(writer, "\nNo slop detected. Suspiciously competent.")?;
        } else {
            writeln!(
                writer,
                "\n{} finding(s). Nice try, robot.",
                result.findings.len()
            )?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct JsonReport<'a> {
    version: &'static str,
    findings: &'a [Finding],
    summary: JsonSummary,
}

#[derive(Serialize)]
struct JsonSummary {
    scanned_files: usize,
    scanned_commits: usize,
    skipped_files: usize,
    findings: usize,
}

/// Writes the structured JSON report.
///
/// # Errors
///
/// Returns an error when serialization or writing fails.
pub fn write_json(mut writer: impl Write, result: &ScanResult) -> serde_json::Result<()> {
    let report = JsonReport {
        version: env!("CARGO_PKG_VERSION"),
        findings: &result.findings,
        summary: JsonSummary {
            scanned_files: result.scanned_files,
            scanned_commits: result.scanned_commits,
            skipped_files: result.skipped_files,
            findings: result.findings.len(),
        },
    };
    serde_json::to_writer_pretty(&mut writer, &report)?;
    writeln!(writer).map_err(serde_json::Error::io)
}

/// Writes a SARIF 2.1.0 report with descriptors for every registered rule.
///
/// # Errors
///
/// Returns an error when serialization or writing fails.
pub fn write_sarif(mut writer: impl Write, result: &ScanResult) -> serde_json::Result<()> {
    let rules: Vec<_> = metadata_registry()
        .into_iter()
        .map(|metadata| {
            json!({
                "id": metadata.id,
                "shortDescription": { "text": metadata.description },
                "fullDescription": { "text": metadata.rationale },
                "help": { "text": metadata.suggestion },
                "defaultConfiguration": { "level": metadata.default_severity.sarif_level() },
                "properties": {
                    "module": metadata.module.to_string(),
                    "confidence": format!("{:?}", metadata.default_confidence).to_ascii_lowercase()
                }
            })
        })
        .collect();
    let results: Vec<_> = result
        .findings
        .iter()
        .map(|finding| {
            json!({
                "ruleId": finding.rule_id,
                "level": finding.severity.sarif_level(),
                "message": { "text": finding.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": artifact_uri(&finding.path) },
                        "region": {
                            "startLine": finding.location.line,
                            "startColumn": finding.location.column,
                            "endLine": finding.location.end_line,
                            "endColumn": finding.location.end_column
                        }
                    }
                }],
                "properties": {
                    "module": finding.module.to_string(),
                    "confidence": format!("{:?}", finding.confidence).to_ascii_lowercase(),
                    "observation": finding.observation
                }
            })
        })
        .collect();
    let report = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "slopcop",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": env!("CARGO_PKG_REPOSITORY"),
                    "rules": rules
                }
            },
            "results": results
        }]
    });
    serde_json::to_writer_pretty(&mut writer, &report)?;
    writeln!(writer).map_err(serde_json::Error::io)
}

/// Writes escaped GitHub workflow command annotations.
///
/// # Errors
///
/// Returns the underlying writer error.
pub fn write_github(mut writer: impl Write, findings: &[Finding]) -> io::Result<()> {
    for finding in findings {
        writeln!(
            writer,
            "::{} file={},line={},col={},title={}::{}",
            match finding.severity {
                Severity::Info => "notice",
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            escape_property(&finding.path.to_string_lossy()),
            finding.location.line,
            finding.location.column,
            finding.rule_id,
            escape_message(&finding.message),
        )?;
    }
    Ok(())
}

/// Writes a GitLab Code Quality report, the Code Climate issue format that merge requests read.
///
/// Fingerprints identify a finding by rule, path, and the text of its line rather than its line
/// number, so GitLab can match findings between the base and head pipelines when nearby edits
/// move them. Repeated identical lines are told apart by their order.
///
/// # Errors
///
/// Returns an error when serialization or writing fails.
pub fn write_gitlab(mut writer: impl Write, findings: &[Finding]) -> serde_json::Result<()> {
    let mut occurrences: HashMap<(&str, String, &str), usize> = HashMap::new();
    let issues: Vec<_> = findings
        .iter()
        .map(|finding| {
            let path = finding.path.to_string_lossy().replace('\\', "/");
            let evidence = finding.evidence.as_deref().unwrap_or_default().trim();
            let occurrence = occurrences
                .entry((finding.rule_id, path.clone(), evidence))
                .or_default();
            *occurrence += 1;
            let fingerprint = fnv1a_128(
                format!("{}\0{path}\0{evidence}\0{occurrence}", finding.rule_id).as_bytes(),
            );
            let description = match &finding.observation {
                Some(observation) => format!("{} Observed: {observation}.", finding.message),
                None => finding.message.clone(),
            };
            json!({
                "type": "issue",
                "description": description,
                "check_name": finding.rule_id,
                "categories": ["Style"],
                "fingerprint": format!("{fingerprint:032x}"),
                "severity": match finding.severity {
                    Severity::Info => "info",
                    Severity::Warning => "minor",
                    Severity::Error => "major",
                },
                "location": {
                    "path": path,
                    "lines": {
                        "begin": finding.location.line,
                        "end": finding.location.end_line
                    }
                }
            })
        })
        .collect();
    serde_json::to_writer_pretty(&mut writer, &issues)?;
    writeln!(writer).map_err(serde_json::Error::io)
}

fn fnv1a_128(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    })
}

fn escape_message(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn artifact_uri(path: &Path) -> String {
    const PATH_ESCAPE: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'%')
        .add(b'<')
        .add(b'>')
        .add(b'?')
        .add(b'`')
        .add(b'{')
        .add(b'}')
        .add(b':')
        .add(b'^')
        .add(b'|')
        .add(b'[')
        .add(b']')
        .add(b'\\');
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.as_bytes().get(1) == Some(&b':') && normalized.as_bytes().get(2) == Some(&b'/') {
        return format!(
            "file:///{}:/{}",
            &normalized[..1],
            utf8_percent_encode(&normalized[3..], PATH_ESCAPE)
        );
    }
    let encoded = utf8_percent_encode(&normalized, PATH_ESCAPE).to_string();
    if normalized.starts_with("//") {
        format!("file:{encoded}")
    } else if normalized.starts_with('/') {
        format!("file://{encoded}")
    } else {
        encoded
    }
}

fn escape_property(value: &str) -> String {
    escape_message(value)
        .replace(':', "%3A")
        .replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Location, Module};

    fn result() -> ScanResult {
        ScanResult {
            scanned_files: 1,
            scanned_commits: 0,
            skipped_files: 0,
            findings: vec![Finding {
                path: "README.md".into(),
                location: Location {
                    line: 4,
                    column: 2,
                    end_line: 4,
                    end_column: 9,
                },
                rule_id: "VIBE001",
                module: Module::Vibecheck,
                severity: Severity::Warning,
                confidence: Confidence::Medium,
                message: "Dense transitions.".to_owned(),
                evidence: None,
                observation: Some("four markers".to_owned()),
                suggestion: "Delete padding.",
            }],
        }
    }

    #[test]
    fn escapes_sarif_uris_and_uses_github_notice_commands() {
        assert_eq!(artifact_uri(Path::new("bad #?.py")), "bad%20%23%3F.py");
        assert_eq!(artifact_uri(Path::new("é%.py")), "%C3%A9%25.py");
        assert_eq!(artifact_uri(Path::new("a:b.py")), "a%3Ab.py");
        assert_eq!(
            artifact_uri(Path::new("/tmp/bad #?.py")),
            "file:///tmp/bad%20%23%3F.py"
        );
        assert_eq!(
            artifact_uri(Path::new("C:\\src\\bad #?.py")),
            "file:///C:/src/bad%20%23%3F.py"
        );
        let mut result = result();
        result.findings[0].severity = Severity::Info;
        result.findings[0].path = "bad,%\n.py".into();
        result.findings[0].message = "message%\nnext".into();
        let mut output = Vec::new();
        write_github(&mut output, &result.findings).expect("GitHub report");
        let output = String::from_utf8(output).expect("UTF-8 report");
        assert!(output.starts_with("::notice file=bad%2C%25%0A.py,"));
        assert!(output.ends_with("::message%25%0Anext\n"));
    }

    #[test]
    fn gitlab_fingerprints_survive_line_moves_and_separate_repeats() {
        let mut result = result();
        result.findings[0].path = "docs\\guide.md".into();
        result.findings[0].evidence = Some("  Moreover, it is robust.".to_owned());
        let mut moved = result.findings[0].clone();
        moved.location.line = 40;
        moved.location.end_line = 40;
        result.findings.push(moved);

        let mut output = Vec::new();
        write_gitlab(&mut output, &result.findings).expect("GitLab report");
        let issues: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
        assert_eq!(issues[0]["check_name"], "VIBE001");
        assert_eq!(issues[0]["severity"], "minor");
        assert_eq!(issues[0]["location"]["path"], "docs/guide.md");
        assert_eq!(issues[0]["location"]["lines"]["end"], 4);
        assert_eq!(
            issues[0]["description"],
            "Dense transitions. Observed: four markers."
        );
        let first = issues[0]["fingerprint"].as_str().expect("fingerprint");
        assert_eq!(first.len(), 32);
        assert_ne!(Some(first), issues[1]["fingerprint"].as_str());

        let mut alone = Vec::new();
        write_gitlab(&mut alone, &result.findings[1..]).expect("GitLab report");
        let alone: serde_json::Value = serde_json::from_slice(&alone).expect("valid JSON");
        assert_eq!(
            alone[0]["fingerprint"], first,
            "a moved finding keeps its fingerprint"
        );
    }

    #[test]
    fn emits_valid_json_and_sarif() {
        let mut json_output = Vec::new();
        write_json(&mut json_output, &result()).expect("JSON report");
        let json: serde_json::Value = serde_json::from_slice(&json_output).expect("valid JSON");
        assert_eq!(json["summary"]["findings"], 1);

        let mut sarif_output = Vec::new();
        write_sarif(&mut sarif_output, &result()).expect("SARIF report");
        let sarif: serde_json::Value =
            serde_json::from_slice(&sarif_output).expect("valid SARIF JSON");
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "VIBE001");
        let region = &sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startColumn"], 2);
        assert_eq!(region["endLine"], 4);
        assert_eq!(region["endColumn"], 9);
        assert_eq!(json["findings"][0]["location"]["end_column"], 9);
    }
}
