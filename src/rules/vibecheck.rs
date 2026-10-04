use std::sync::OnceLock;

use regex::Regex;

use super::{Rule, ScanContext};
use crate::language::SourceType;
use crate::model::{Confidence, Finding, Module, RuleMetadata, Severity};

pub struct FormulaicTransitionDensity;

const TRANSITIONS: &[&str] = &[
    r"\bultimately\b",
    r"\bthat said\b",
    r"\bat its core\b",
    r"\bwith that in mind\b",
    r"\bthe key is\b",
    r"\bin other words\b",
    r"\bit is worth noting\b",
    r"\bin today's .{0,24} landscape\b",
];

static METADATA: RuleMetadata = RuleMetadata {
    id: "VIBE001",
    module: Module::Vibecheck,
    description: "Dense formulaic transitions",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Formulaic transitions are unusually dense.",
    suggestion: "Remove signposts that do not add meaning and connect the concrete claims directly.",
    rationale: "Repeated stock transitions pad prose and make its structure more predictable without adding information.",
    examples: &["Ultimately ... That said ... At its core ... With that in mind ..."],
    false_positives: "Long-form teaching material and formal argument may legitimately use more transitions; one phrase never triggers this rule.",
};

impl Rule for FormulaicTransitionDensity {
    fn metadata(&self) -> &'static RuleMetadata {
        &METADATA
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        if matches!(
            context.source_type,
            SourceType::Configuration | SourceType::Unknown
        ) {
            return;
        }

        let prose = context.prose();
        let words = prose.split_whitespace().count();
        let mut matches = transition_matcher().find_iter(prose);
        let Some(first) = matches.next() else {
            return;
        };
        let count = 1 + matches.count();
        if count < 4 || count.saturating_mul(120) < words.max(1) {
            return;
        }

        let first_offset = first.start();
        findings.push(Finding {
            path: context.path.to_path_buf(),
            location: context.location(first_offset),
            rule_id: METADATA.id,
            module: METADATA.module,
            severity: METADATA.default_severity,
            confidence: METADATA.default_confidence,
            message: METADATA.message.to_owned(),
            evidence: Some(context.line_at(first_offset)),
            observation: Some(format!(
                "observed {count} transition markers across {words} words"
            )),
            suggestion: METADATA.suggestion,
        });
    }
}

fn transition_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(&format!("(?i){}", TRANSITIONS.join("|")))
            .expect("VIBE001 transition regexes must compile")
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn detects_cluster_but_not_single_phrase() {
        let noisy = "Ultimately, choose one. That said, test it. At its core, this is simple. With that in mind, ship it.";
        let clean = "Ultimately, the measured latency determines whether this approach is viable.";

        for (source, expected) in [(noisy, 1), (clean, 0)] {
            let context =
                ScanContext::new(Path::new("README.md"), source, SourceType::Documentation);
            let mut findings = Vec::new();
            FormulaicTransitionDensity.check(&context, &mut findings);
            assert_eq!(findings.len(), expected);
        }
    }

    #[test]
    fn scans_code_comments_but_not_strings() {
        let source = concat!(
            "const hidden = \"Ultimately. That said. At its core. With that in mind.\";\n",
            "// Ultimately, measure it.\n",
            "// That said, test it.\n",
            "// At its core, this is code.\n",
            "// With that in mind, ship it.\n",
        );
        let context = ScanContext::new(
            Path::new("service.ts"),
            source,
            SourceType::Code(crate::language::Language::TypeScript),
        );
        let mut findings = Vec::new();

        FormulaicTransitionDensity.check(&context, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location.line, 2);
    }
}
