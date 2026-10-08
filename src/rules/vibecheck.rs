use std::sync::OnceLock;

use regex::Regex;

use super::vibecheck_extra::{inside_double_quotes, with_replacements};
use super::{Rule, ScanContext};
use crate::language::SourceType;
use crate::model::{Confidence, Finding, Module, RuleMetadata, Severity};

pub struct FormulaicTransitionDensity;

/// Stock transitions and pivot phrases that announce a point instead of making it. Each entry is
/// a case-insensitive regex fragment; `'` also matches a typographic apostrophe.
const TRANSITIONS: &[&str] = &[
    r"\bultimately\b",
    r"\bthat said\b",
    r"\bat its core\b",
    r"\bat a high level\b",
    r"\bwith that in mind\b",
    r"\bthe key(?: here| insight)? is (?:to|that|not|simple|understanding|knowing|recognizing)\b",
    r"\bin other words\b",
    r"\bput (?:differently|another way)\b",
    r"\btaken together\b",
    r"\bit(?:'s| is) worth noting\b",
    r"\bit(?:'s| is) important to (?:recognize|remember|understand)\b",
    r"\bhere(?:'s| is) the thing\b",
    r"\bthe (?:reality|truth|bottom line) is\b",
    r"\bthe real question is\b",
    r"\bwhat (?:really )?matters(?: most)? is\b",
    r"\bwhat this means is\b",
    r"\bthe (?:broader|bigger|deeper|underlying) (?:point|issue|problem|lesson)(?: here)? is\b",
    r"\bthe (?:important|key|crucial) distinction(?: here)? is\b",
    r"\b(?:that|this) distinction matters\b",
    r"\bthis (?:really )?matters because\b",
    r"\bthis is (?:especially|particularly) important because\b",
    r"\bthe (?:key )?(?:takeaway|lesson)(?: here)? is\b",
    r"\ba (?:useful|helpful|good) way to think about (?:it|this)\b",
    r"\bthe simplest way to (?:understand|think about) (?:it|this)\b",
    r"\bit (?:all )?comes down to\b",
    r"\bthis raises an important question\b",
    r"\b(?:that|this) brings us to\b",
    r"\bthis is where (?:things|it) gets?\b",
    r"\bmore broadly\b",
    r"\b(?:fundamentally|(?:perhaps )?(?:more |most )?importantly|notably|to be clear),",
    r"\bfrom this perspective\b",
    r"\bin today's .{0,24} landscape\b",
    r"\b(?:moreover|furthermore|additionally),",
    r"\bin essence\b",
    r"\bthat being said\b",
    r"\bto put it simply\b",
    r"\ball things considered\b",
    r"\bwhen it comes to\b",
    r"\bthis begs the question\b",
    r"\bthis is where [a-z ]{1,30} comes in\b",
    r"\bhere(?:'s| is) (?:the (?:kicker|deal|catch)|where (?:it|things) gets?)\b",
    r"\bplot twist\b",
    r"\bwhat (?:nobody|no one) tells you\b",
    r"\bthe part (?:everyone|nobody|most people) (?:misses|overlooks)\b",
    r"\bwhat if i told you\b",
];

/// Plain alternatives for the transitions that have one. Most signposts are best deleted, so the
/// replacement says so.
const TRANSITION_REPLACEMENTS: &[(&str, &str)] = &[
    ("ultimately", "cut it"),
    ("that said", "but, still"),
    ("that being said", "but, still"),
    ("at its core", "cut it"),
    ("in essence", "cut it"),
    ("at a high level", "cut it, or say \"in short\""),
    ("with that in mind", "so"),
    ("in other words", "cut it and say it once, clearly"),
    ("put differently", "cut it and say it once, clearly"),
    ("put another way", "cut it and say it once, clearly"),
    ("to put it simply", "cut it and say it simply"),
    ("taken together", "cut it"),
    ("all things considered", "cut it"),
    ("more broadly", "cut it"),
    ("moreover,", "also, and"),
    ("furthermore,", "also, and"),
    ("additionally,", "also"),
    ("notably,", "cut it"),
    ("importantly,", "cut it"),
    ("fundamentally,", "cut it"),
    ("to be clear,", "cut it"),
    ("when it comes to", "for, about"),
    ("it comes down to", "depends on"),
    ("it all comes down to", "depends on"),
    ("here's the thing", "cut it"),
    ("here is the thing", "cut it"),
    ("here's the kicker", "cut it"),
    ("here's the deal", "cut it"),
    ("here's the catch", "but"),
    ("plot twist", "cut it"),
    (
        "this begs the question",
        "this raises the question, or ask it",
    ),
    ("from this perspective", "cut it"),
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
    examples: &[
        "Ultimately ... That said ... At its core ... With that in mind ...",
        "The real question is ... Put differently ... The bottom line is ... Let's unpack that.",
        "Moreover, ... Furthermore, ... In essence ... Here's the kicker: ...",
    ],
    false_positives: "Long-form teaching material and formal argument may legitimately use more transitions; one phrase never triggers this rule, and double-quoted examples are excluded.",
    replacements: TRANSITION_REPLACEMENTS,
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
        let matches: Vec<_> = transition_matcher()
            .find_iter(prose)
            .filter(|found| !inside_double_quotes(prose, found.start()))
            .collect();
        let count = matches.len();
        if count < 4 || count.saturating_mul(120) < words.max(1) {
            return;
        }

        let first_offset = matches[0].start();
        let expressions: Vec<String> = matches
            .iter()
            .map(|found| found.as_str().trim_end_matches(',').to_lowercase())
            .collect();
        findings.push(Finding {
            path: context.path.to_path_buf(),
            location: context.location(first_offset),
            rule_id: METADATA.id,
            module: METADATA.module,
            severity: METADATA.default_severity,
            confidence: METADATA.default_confidence,
            message: METADATA.message.to_owned(),
            evidence: Some(context.line_at(first_offset)),
            observation: Some(with_replacements(
                &METADATA,
                format!("observed {count} transition markers across {words} words"),
                expressions.iter().map(String::as_str),
            )),
            suggestion: METADATA.suggestion,
        });
    }
}

fn transition_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        // ASCII word boundaries keep the regex on its DFA engines; a Unicode `\b` falls back to a
        // slower engine whenever the prose holds a non-ASCII character, and every phrase is ASCII.
        let pattern = TRANSITIONS
            .join("|")
            .replace('\'', "['\u{2019}]")
            .replace(r"\b", r"(?-u:\b)");
        Regex::new(&format!("(?i){pattern}")).expect("VIBE001 transition regexes must compile")
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

    fn count(source: &str) -> usize {
        let context = ScanContext::new(Path::new("README.md"), source, SourceType::Documentation);
        let mut findings = Vec::new();
        FormulaicTransitionDensity.check(&context, &mut findings);
        findings.len()
    }

    #[test]
    fn detects_pivot_phrases_with_either_apostrophe() {
        assert_eq!(
            count(
                "The real question is scope. Put differently, ownership. Here\u{2019}s the thing: nobody reads it. The bottom line is that the docs need an owner."
            ),
            1
        );
        assert_eq!(
            count(
                "Notably, the steps matter. More broadly, what matters is the order. It\u{2019}s worth noting that it comes down to timing."
            ),
            1
        );
    }

    #[test]
    fn detects_reference_transitions_and_names_replacements() {
        let source = "Moreover, the parser is small. Furthermore, it is fast. In essence, it works. Here's the kicker: it is free.";
        let context = ScanContext::new(Path::new("README.md"), source, SourceType::Documentation);
        let mut findings = Vec::new();
        FormulaicTransitionDensity.check(&context, &mut findings);
        let observation = findings[0].observation.as_deref().unwrap_or_default();
        assert!(
            observation.contains(r#""moreover", "furthermore" -> also, and"#),
            "{observation}"
        );
    }

    #[test]
    fn ignores_quoted_examples_and_literal_keys() {
        assert_eq!(
            count(
                r#"Avoid "the real question is", "put differently", "taken together", and "the bottom line is" in reviews."#
            ),
            0
        );
        assert_eq!(
            count(
                "The key is stored in the keyring. The key is rotated weekly. The key is never logged. The key is 32 bytes."
            ),
            0
        );
        assert_eq!(
            count(
                "Writes are notably faster. Reads are fundamentally bounded. Importantly sized pages fit. To be clear errors are logged."
            ),
            0
        );
    }
}
