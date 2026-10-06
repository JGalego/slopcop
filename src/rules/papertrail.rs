use std::path::Path;

use crate::model::{Confidence, Finding, Location, Module, RuleMetadata, Severity};

pub(crate) struct CommitContext<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub from_history: bool,
    subject: Option<(usize, &'a str)>,
}

impl<'a> CommitContext<'a> {
    pub(crate) fn new(path: &'a Path, source: &'a str, from_history: bool) -> Self {
        Self {
            path,
            source,
            from_history,
            subject: commit_subject(source),
        }
    }

    fn subject(&self) -> Option<(usize, &str)> {
        self.subject
    }

    fn location(&self, offset: usize) -> Location {
        Location::at(self.source, offset)
    }
}

type CheckFn = fn(&CommitContext<'_>, &'static RuleMetadata, &mut Vec<Finding>);

pub(crate) struct PapertrailRule {
    metadata: &'static RuleMetadata,
    history_only: bool,
    check_fn: CheckFn,
}

impl PapertrailRule {
    pub(crate) const fn metadata(&self) -> &'static RuleMetadata {
        self.metadata
    }

    pub(crate) fn check(&self, context: &CommitContext<'_>, findings: &mut Vec<Finding>) {
        if !self.history_only || context.from_history {
            (self.check_fn)(context, self.metadata, findings);
        }
    }
}

static TRAIL001: RuleMetadata = RuleMetadata {
    id: "TRAIL001",
    module: Module::Papertrail,
    description: "Placeholder commit subject",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "The commit subject is a placeholder rather than a change summary.",
    suggestion: "Summarize the observable change with a specific imperative subject.",
    rationale: "Subjects such as WIP, fix, or update do not preserve why a revision exists and make history harder to search or review.",
    examples: &["WIP", "fix", "update"],
    false_positives: "Temporary local commits can be useful; disable or demote this rule on personal branches that are always squashed.",
};

static TRAIL002: RuleMetadata = RuleMetadata {
    id: "TRAIL002",
    module: Module::Papertrail,
    description: "Autosquash commit left in history",
    default_severity: Severity::Error,
    default_confidence: Confidence::High,
    message: "An autosquash commit remains in the inspected history.",
    suggestion: "Run an autosquash rebase before merging the branch.",
    rationale: "Fixup, squash, and amend commits are editing instructions for a later rebase, not durable history entries.",
    examples: &["fixup! Handle empty input", "squash! Add scanner cache"],
    false_positives: "These commits are expected while preparing a series; run history checks at the integration boundary.",
};

pub(crate) fn registry() -> Vec<PapertrailRule> {
    vec![
        PapertrailRule {
            metadata: &TRAIL001,
            history_only: false,
            check_fn: check_placeholder_subject,
        },
        PapertrailRule {
            metadata: &TRAIL002,
            history_only: true,
            check_fn: check_autosquash_subject,
        },
    ]
}

fn check_placeholder_subject(
    context: &CommitContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<Finding>,
) {
    let (offset, subject) = context.subject().unwrap_or((0, ""));
    let normalized = subject
        .trim_matches(|character: char| !character.is_alphanumeric())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "wip"
                | "work in progress"
                | "fix"
                | "update"
                | "changes"
                | "misc"
                | "tmp"
                | "temp"
                | "checkpoint"
        )
    {
        emit(
            context,
            metadata,
            findings,
            offset,
            if subject.is_empty() {
                "<empty subject>"
            } else {
                subject
            },
        );
    }
}

fn check_autosquash_subject(
    context: &CommitContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<Finding>,
) {
    let Some((offset, subject)) = context.subject() else {
        return;
    };
    let lower = subject.to_ascii_lowercase();
    if ["fixup!", "squash!", "amend!"].iter().any(|prefix| {
        lower
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
    }) {
        emit(context, metadata, findings, offset, subject);
    }
}

fn emit(
    context: &CommitContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<Finding>,
    offset: usize,
    evidence: &str,
) {
    findings.push(Finding {
        path: context.path.to_path_buf(),
        location: context.location(offset),
        rule_id: metadata.id,
        module: metadata.module,
        severity: metadata.default_severity,
        confidence: metadata.default_confidence,
        message: metadata.message.to_owned(),
        evidence: Some(evidence.to_owned()),
        observation: None,
        suggestion: metadata.suggestion,
    });
}

fn commit_subject(source: &str) -> Option<(usize, &str)> {
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let leading = content.len() - content.trim_start().len();
            return Some((offset + leading, trimmed));
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(source: &str, from_history: bool) -> Vec<Finding> {
        let context = CommitContext::new(Path::new("COMMIT_EDITMSG"), source, from_history);
        let mut findings = Vec::new();
        for rule in registry() {
            rule.check(&context, &mut findings);
        }
        findings
    }

    #[test]
    fn placeholder_subjects_require_a_specific_summary() {
        assert_eq!(findings("WIP\n", false)[0].rule_id, "TRAIL001");
        assert_eq!(findings("Update.\n", false)[0].rule_id, "TRAIL001");
        assert_eq!(findings("# Template only\n", false)[0].rule_id, "TRAIL001");
        assert_eq!(
            findings("Handle empty scanner input\n", false),
            [] as [Finding; 0]
        );
    }

    #[test]
    fn commit_templates_preserve_subject_locations() {
        let findings = findings("# Explain the change\n\n  fix\n", false);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].location,
            Location {
                line: 3,
                column: 3,
                end_line: 3,
                end_column: 6
            }
        );
    }

    #[test]
    fn autosquash_markers_only_fail_history_checks() {
        assert_eq!(
            findings("fixup! Handle empty input\n", false),
            [] as [Finding; 0]
        );
        let findings = findings("fixup! Handle empty input\n", true);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "TRAIL002");
    }
}
