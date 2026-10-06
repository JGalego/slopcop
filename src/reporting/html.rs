use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::model::{Finding, RuleMetadata, Severity};
use crate::rules::metadata_registry;
use crate::scanner::ScanResult;

/// Optional details that an HTML report shows alongside the findings.
#[derive(Clone, Copy, Debug, Default)]
pub struct HtmlContext<'a> {
    /// Names what was scanned, such as a repository and commit.
    pub title: Option<&'a str>,
    /// Prefix for source links; a finding links to `{source_url}{path}#L{line}`.
    pub source_url: Option<&'a str>,
    /// Notes about the scan, such as files that were left out.
    pub notes: &'a [String],
}

const SEVERITIES: [Severity; 3] = [Severity::Error, Severity::Warning, Severity::Info];

const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

const STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #f6f6f1; --surface: #ffffff; --surface-2: #f0f0ea; --ink: #171717; --muted: #66665f;
  --line: #e2e2da; --lime: #c7ff3d; --error: #c62828; --warning: #a35f00; --info: #2a62c9;
  --mono: ui-monospace, "SF Mono", SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
  --sans: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", sans-serif;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0f0f0e; --surface: #171716; --surface-2: #1f1f1d; --ink: #ecece6; --muted: #9b9b93;
    --line: #2c2c29; --error: #ff7a6e; --warning: #f2b440; --info: #79a8ff;
  }
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink); font: 15px/1.5 var(--sans); }
a { color: inherit; }
code, pre { font-family: var(--mono); }
.masthead { background: #171717; color: #f6f6f1; }
.wrap { max-width: 1080px; margin: 0 auto; padding: 0 16px; }
.masthead .wrap { display: flex; justify-content: space-between; align-items: baseline; padding-block: 10px; }
.brand { font-weight: 700; font-size: 17px; }
.brand::before { content: ""; display: inline-block; width: 10px; height: 10px; margin-right: 8px; border-radius: 50%; background: var(--lime); }
.version { font: 13px var(--mono); opacity: 0.7; }
h1 { font-size: 26px; letter-spacing: -0.02em; margin: 32px 0 16px; overflow-wrap: anywhere; }
h2 { font-size: 18px; margin: 36px 0 12px; }
.stats { display: flex; flex-wrap: wrap; gap: 8px 28px; margin: 0; padding: 16px; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
.stats div { display: flex; flex-direction: column; }
.stats dt { font-size: 22px; font-weight: 700; font-variant-numeric: tabular-nums; }
.stats dd { margin: 0; font-size: 13px; color: var(--muted); }
.stats .error dt { color: var(--error); }
.stats .warning dt { color: var(--warning); }
.stats .info dt { color: var(--info); }
.notes { margin: 12px 0 0; padding-left: 20px; color: var(--muted); font-size: 14px; }
.empty { margin: 32px 0; padding: 24px; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
table { width: 100%; border-collapse: collapse; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; font-size: 14px; }
th, td { text-align: left; padding: 7px 12px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
th:last-child, td.count { text-align: right; font-variant-numeric: tabular-nums; }
.filters { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; margin-bottom: 12px; font-size: 14px; color: var(--muted); }
.filters label { display: inline-flex; gap: 6px; align-items: center; cursor: pointer; color: var(--ink); }
.file { margin-bottom: 14px; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; overflow: hidden; }
.file > summary { display: flex; gap: 10px; align-items: center; padding: 9px 14px; background: var(--surface-2); cursor: pointer; font: 13px var(--mono); }
.file > summary .path { font-weight: 600; overflow-wrap: anywhere; }
.file > summary a { margin-left: auto; font-family: var(--sans); color: var(--muted); }
.count { color: var(--muted); font-size: 12px; }
.finding { padding: 12px 14px; border-top: 1px solid var(--line); border-left: 3px solid transparent; }
.finding.sev-error { border-left-color: var(--error); }
.finding.sev-warning { border-left-color: var(--warning); }
.finding.sev-info { border-left-color: var(--info); }
.finding-head { display: flex; flex-wrap: wrap; gap: 4px 10px; align-items: baseline; }
.badge { font: 600 11px var(--mono); text-transform: uppercase; letter-spacing: 0.04em; }
.badge.sev-error { color: var(--error); }
.badge.sev-warning { color: var(--warning); }
.badge.sev-info { color: var(--info); }
.rule-id { font: 600 13px var(--mono); }
.message { flex: 1; min-width: 12em; }
.location { font: 12px var(--mono); color: var(--muted); }
.finding p { margin: 6px 0 0; font-size: 14px; }
.observation { color: var(--muted); }
.suggestion span { font-weight: 600; }
pre { margin: 8px 0 0; padding: 8px 10px; background: var(--surface-2); border-radius: 6px; font-size: 13px; white-space: pre-wrap; overflow-wrap: anywhere; }
.rule { margin-bottom: 14px; padding: 14px 16px; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; scroll-margin-top: 16px; }
.rule h3 { margin: 0 0 4px; font: 600 15px var(--mono); }
.rule h3 span { font: 13px var(--sans); color: var(--muted); margin-left: 8px; }
.rule h4 { margin: 12px 0 2px; font-size: 13px; }
.rule p { margin: 4px 0 0; font-size: 14px; }
.rule .hint { color: var(--muted); }
footer { margin: 48px auto 32px; font-size: 13px; color: var(--muted); }
body:has(#show-error:not(:checked)) .finding.sev-error,
body:has(#show-warning:not(:checked)) .finding.sev-warning,
body:has(#show-info:not(:checked)) .finding.sev-info,
body:has(.filters input:not(:checked)) .file { display: none; }
body:has(#show-error:checked) .file:has(.finding.sev-error),
body:has(#show-warning:checked) .file:has(.finding.sev-warning),
body:has(#show-info:checked) .file:has(.finding.sev-info) { display: block; }
@media print {
  .masthead, .filters, .file > summary a { display: none; }
  .file, .rule { break-inside: avoid; }
}
"#;

/// Writes a self-contained HTML report with no scripts or external resources.
///
/// # Errors
///
/// Returns the underlying writer error.
pub fn write_html(
    mut writer: impl Write,
    result: &ScanResult,
    context: &HtmlContext<'_>,
) -> io::Result<()> {
    let rules: BTreeMap<&str, &RuleMetadata> = metadata_registry()
        .into_iter()
        .map(|metadata| (metadata.id, metadata))
        .collect();
    let version = env!("CARGO_PKG_VERSION");
    let heading = context.title.unwrap_or("slopcop report");

    writeln!(
        writer,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"generator\" content=\"slopcop {version}\">\n<title>{}</title>\n\
         <style>{STYLE}</style>\n</head>\n<body>\n\
         <header class=\"masthead\"><div class=\"wrap\"><span class=\"brand\">slopcop</span>\
         <span class=\"version\">{version}</span></div></header>\n<main class=\"wrap\">\n<h1>{}</h1>",
        Escaped(
            &context
                .title
                .map_or_else(|| heading.to_owned(), |title| format!("{title} · slopcop"))
        ),
        Escaped(heading),
    )?;
    write_summary(&mut writer, result, context)?;

    if result.findings.is_empty() {
        writeln!(
            writer,
            "<p class=\"empty\"><strong>No slop detected.</strong> Suspiciously competent.</p>"
        )?;
    } else {
        write_rule_counts(&mut writer, &result.findings, &rules)?;
        write_findings(&mut writer, &result.findings, context)?;
        write_reference(&mut writer, &result.findings, &rules)?;
    }

    writeln!(
        writer,
        "</main>\n<footer class=\"wrap\">Generated by <a href=\"{}\">slopcop</a> {version}.</footer>\n</body>\n</html>",
        Escaped(env!("CARGO_PKG_REPOSITORY")),
    )
}

fn write_summary(
    writer: &mut impl Write,
    result: &ScanResult,
    context: &HtmlContext<'_>,
) -> io::Result<()> {
    let findings = result.findings.len();
    writeln!(writer, "<dl class=\"stats\">")?;
    stat(
        writer,
        "",
        findings,
        if findings == 1 { "finding" } else { "findings" },
    )?;
    for severity in SEVERITIES {
        let count = count_severity(&result.findings, severity);
        stat(writer, &severity.to_string(), count, &severity.to_string())?;
    }
    stat(writer, "", result.scanned_files, "files scanned")?;
    if result.scanned_commits > 0 {
        stat(writer, "", result.scanned_commits, "commits scanned")?;
    }
    stat(writer, "", result.skipped_files, "skipped")?;
    writeln!(writer, "</dl>")?;

    if !context.notes.is_empty() {
        writeln!(writer, "<ul class=\"notes\">")?;
        for note in context.notes {
            writeln!(writer, "<li>{}</li>", Escaped(note))?;
        }
        writeln!(writer, "</ul>")?;
    }
    Ok(())
}

fn stat(writer: &mut impl Write, class: &str, value: usize, label: &str) -> io::Result<()> {
    writeln!(
        writer,
        "<div class=\"{class}\"><dt>{value}</dt><dd>{}</dd></div>",
        Escaped(label)
    )
}

fn count_severity(findings: &[Finding], severity: Severity) -> usize {
    findings
        .iter()
        .filter(|finding| finding.severity == severity)
        .count()
}

fn write_rule_counts(
    writer: &mut impl Write,
    findings: &[Finding],
    rules: &BTreeMap<&str, &RuleMetadata>,
) -> io::Result<()> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for finding in findings {
        *counts.entry(finding.rule_id).or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));

    writeln!(
        writer,
        "<h2>Rules</h2>\n<table>\n<thead><tr><th>Rule</th><th>Module</th><th>Description</th><th>Findings</th></tr></thead>\n<tbody>"
    )?;
    for (id, count) in counts {
        let metadata = rules.get(id);
        writeln!(
            writer,
            "<tr><td><a class=\"rule-id\" href=\"#rule-{id}\">{id}</a></td><td>{}</td><td>{}</td><td class=\"count\">{count}</td></tr>",
            metadata
                .map(|metadata| metadata.module.to_string())
                .unwrap_or_default(),
            Escaped(metadata.map_or("", |metadata| metadata.description)),
        )?;
    }
    writeln!(writer, "</tbody>\n</table>")
}

fn write_findings(
    writer: &mut impl Write,
    findings: &[Finding],
    context: &HtmlContext<'_>,
) -> io::Result<()> {
    writeln!(
        writer,
        "<h2>Findings</h2>\n<div class=\"filters\" role=\"group\" aria-label=\"Show severities\">Show"
    )?;
    for severity in SEVERITIES {
        writeln!(
            writer,
            "<label><input type=\"checkbox\" id=\"show-{severity}\" checked> {severity} ({})</label>",
            count_severity(findings, severity),
        )?;
    }
    writeln!(writer, "</div>")?;

    for group in findings.chunk_by(|left, right| left.path == right.path) {
        let path = group[0].path.to_string_lossy();
        write!(
            writer,
            "<details class=\"file\" open>\n<summary><span class=\"path\">{}</span><span class=\"count\">{}</span>",
            Escaped(&path),
            group.len(),
        )?;
        if let Some(base) = context.source_url {
            write!(
                writer,
                "<a href=\"{}\">Source</a>",
                Escaped(&source_link(base, &path, None)),
            )?;
        }
        writeln!(writer, "</summary>")?;
        for finding in group {
            write_finding(writer, finding, &path, context)?;
        }
        writeln!(writer, "</details>")?;
    }
    Ok(())
}

fn write_finding(
    writer: &mut impl Write,
    finding: &Finding,
    path: &str,
    context: &HtmlContext<'_>,
) -> io::Result<()> {
    let severity = finding.severity;
    let id = finding.rule_id;
    let line = finding.location.line;
    let column = finding.location.column;
    write!(
        writer,
        "<article class=\"finding sev-{severity}\">\n<div class=\"finding-head\">\
         <span class=\"badge sev-{severity}\">{severity}</span>\
         <a class=\"rule-id\" href=\"#rule-{id}\">{id}</a>\
         <span class=\"message\">{}</span>",
        Escaped(&finding.message),
    )?;
    match context.source_url {
        Some(base) => write!(
            writer,
            "<a class=\"location\" href=\"{}\">{line}:{column}</a>",
            Escaped(&source_link(base, path, Some(line))),
        )?,
        None => write!(writer, "<span class=\"location\">{line}:{column}</span>")?,
    }
    writeln!(writer, "</div>")?;
    if let Some(observation) = &finding.observation {
        writeln!(
            writer,
            "<p class=\"observation\">{}</p>",
            Escaped(observation)
        )?;
    }
    if let Some(evidence) = &finding.evidence {
        writeln!(
            writer,
            "<pre class=\"evidence\">{}</pre>",
            Escaped(evidence.trim_end())
        )?;
    }
    writeln!(
        writer,
        "<p class=\"suggestion\"><span>Fix:</span> {}</p>\n</article>",
        Escaped(finding.suggestion),
    )
}

fn write_reference(
    writer: &mut impl Write,
    findings: &[Finding],
    rules: &BTreeMap<&str, &RuleMetadata>,
) -> io::Result<()> {
    let fired: BTreeSet<&str> = findings.iter().map(|finding| finding.rule_id).collect();
    writeln!(writer, "<h2>Rule reference</h2>")?;
    for metadata in fired.iter().filter_map(|id| rules.get(id)) {
        let id = metadata.id;
        writeln!(
            writer,
            "<article class=\"rule\" id=\"rule-{id}\">\n<h3>{id}<span>{} · default {}, {} confidence</span></h3>\n\
             <p>{}</p>\n<h4>Why it matters</h4>\n<p>{}</p>",
            metadata.module,
            metadata.default_severity,
            format!("{:?}", metadata.default_confidence).to_ascii_lowercase(),
            Escaped(metadata.description),
            Escaped(metadata.rationale),
        )?;
        if !metadata.examples.is_empty() {
            writeln!(writer, "<h4>Examples</h4>")?;
            for example in metadata.examples {
                writeln!(writer, "<pre>{}</pre>", Escaped(example))?;
            }
        }
        writeln!(
            writer,
            "<h4>Fix</h4>\n<p>{}</p>\n<h4>False positives</h4>\n<p>{}</p>\n\
             <p class=\"hint\">Suppress with a reason: <code>slopcop: ignore {id} -- reason</code></p>\n</article>",
            Escaped(metadata.suggestion),
            Escaped(metadata.false_positives),
        )?;
    }
    Ok(())
}

fn source_link(base: &str, path: &str, line_number: Option<usize>) -> String {
    let encoded: Vec<String> = path
        .replace('\\', "/")
        .split('/')
        .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT).to_string())
        .collect();
    let encoded = encoded.join("/");
    match line_number {
        Some(line) => format!("{base}{encoded}#L{line}"),
        None => format!("{base}{encoded}"),
    }
}

/// Escapes text for HTML element content and quoted attribute values.
struct Escaped<'a>(&'a str);

impl fmt::Display for Escaped<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut rest = self.0;
        while let Some(index) = rest.find(['&', '<', '>', '"', '\'']) {
            formatter.write_str(&rest[..index])?;
            formatter.write_str(match rest.as_bytes()[index] {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                b'"' => "&quot;",
                _ => "&#39;",
            })?;
            rest = &rest[index + 1..];
        }
        formatter.write_str(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Location, Module};

    fn finding(path: &str, line: usize, rule_id: &'static str, severity: Severity) -> Finding {
        Finding {
            path: path.into(),
            location: Location { line, column: 1 },
            rule_id,
            module: Module::Deadweight,
            severity,
            confidence: Confidence::High,
            message: "Empty <catch> & \"friends\".".to_owned(),
            evidence: Some("    except: pass  \n".to_owned()),
            observation: None,
            suggestion: "Handle it.",
        }
    }

    fn render(result: &ScanResult, context: &HtmlContext<'_>) -> String {
        let mut output = Vec::new();
        write_html(&mut output, result, context).expect("HTML report");
        String::from_utf8(output).expect("UTF-8 report")
    }

    #[test]
    fn escapes_text_and_groups_findings_by_file() {
        let result = ScanResult {
            scanned_files: 2,
            scanned_commits: 0,
            skipped_files: 0,
            findings: vec![
                finding("a <b>.py", 3, "DEAD001", Severity::Error),
                finding("a <b>.py", 9, "DEAD001", Severity::Warning),
                finding("c.py", 1, "DEAD002", Severity::Warning),
            ],
        };
        let output = render(&result, &HtmlContext::default());
        assert!(output.starts_with("<!doctype html>"));
        assert!(output.trim_end().ends_with("</html>"));
        assert!(!output.contains("<script"));
        assert!(
            output.contains(
                "<span class=\"path\">a &lt;b&gt;.py</span><span class=\"count\">2</span>"
            )
        );
        assert!(output.contains("Empty &lt;catch&gt; &amp; &quot;friends&quot;."));
        assert!(output.contains("<pre class=\"evidence\">    except: pass</pre>"));
        assert!(output.contains("<span class=\"location\">9:1</span>"));
        assert_eq!(output.matches("<details class=\"file\"").count(), 2);
        assert_eq!(output.matches("<article class=\"rule\"").count(), 2);
        assert!(output.contains("id=\"rule-DEAD001\""));
        assert!(output.contains("<title>slopcop report</title>"));
    }

    #[test]
    fn links_sources_and_lists_notes() {
        let result = ScanResult {
            scanned_files: 1,
            scanned_commits: 0,
            skipped_files: 0,
            findings: vec![finding("docs/read me#1.md", 4, "DEAD001", Severity::Error)],
        };
        let notes = ["Left out <3> files.".to_owned()];
        let output = render(
            &result,
            &HtmlContext {
                title: Some("owner/repo · abc1234"),
                source_url: Some("https://github.com/owner/repo/blob/abc/"),
                notes: &notes,
            },
        );
        assert!(output.contains("<title>owner/repo · abc1234 · slopcop</title>"));
        assert!(output.contains(
            "href=\"https://github.com/owner/repo/blob/abc/docs/read%20me%231.md#L4\">4:1</a>"
        ));
        assert!(output.contains("<li>Left out &lt;3&gt; files.</li>"));
    }

    #[test]
    fn reports_a_clean_scan() {
        let result = ScanResult {
            scanned_files: 5,
            scanned_commits: 0,
            skipped_files: 1,
            findings: Vec::new(),
        };
        let output = render(&result, &HtmlContext::default());
        assert!(output.contains("No slop detected."));
        assert!(!output.contains("<h2>Findings</h2>"));
        assert!(output.contains("<dt>5</dt><dd>files scanned</dd>"));
    }
}
