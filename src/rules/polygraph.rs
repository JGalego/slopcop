//! Metadata for the optional polygraph module. The checks live in `crate::polygraph` and need the
//! `polygraph` feature and a model file; the metadata is always present so that configuration,
//! `slopcop explain`, and the rule index behave the same in every build.

use crate::model::{Confidence, Module, RuleMetadata, Severity};

pub static POLY001: RuleMetadata = RuleMetadata {
    id: "POLY001",
    module: Module::Polygraph,
    description: "Sustained predictable prose",
    default_severity: Severity::Info,
    default_confidence: Confidence::Low,
    message: "A run of sentences is unusually easy for a language model to predict.",
    suggestion: "Replace the predictable sentences with the specific fact, number, or example they stand in for.",
    rationale: "Prose made of the most expected word at every step carries little information. The rule runs a small language model and reports the sentences it found most predictable, with their mean surprisal, so a reader can judge whether they say anything. It measures the text, not the writer: formulaic human writing scores the same way.",
    examples: &[
        "In today's world, it is important to remember that quality matters. Quality is something that matters in every project. Every project benefits when quality is a priority.",
    ],
    false_positives: "Boilerplate, legal text, specifications, controlled language, and writing by non-native speakers can all be highly predictable. Short documents are not scored. The rule runs only when a language model is configured, and results are reproducible on one build and CPU family, not bit for bit across machines.",
    replacements: &[],
};

pub static POLY002: RuleMetadata = RuleMetadata {
    id: "POLY002",
    module: Module::Polygraph,
    description: "Flat surprisal",
    default_severity: Severity::Info,
    default_confidence: Confidence::Low,
    message: "Sentence predictability barely varies across the document.",
    suggestion: "Look for places where the text could be more specific, shorter, or less even.",
    rationale: "Prose usually alternates between expected and surprising sentences. A document whose sentences are almost equally predictable from start to finish lacks that variation. The finding reports the spread of per-sentence surprisal and the sentences at both extremes.",
    examples: &["A 40-sentence guide in which every sentence has nearly the same mean surprisal."],
    false_positives: "Reference tables, glossaries, API listings, and other uniform formats are flat by design. Short documents are not scored. The rule runs only when a language model is configured.",
    replacements: &[],
};

pub static POLY003: RuleMetadata = RuleMetadata {
    id: "POLY003",
    module: Module::Polygraph,
    description: "Comment restates the code",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "The comment says what the next line already says.",
    suggestion: "Delete the comment or replace it with the reason the code does what it does.",
    rationale: "A static embedding of the comment and of the identifiers on the line below it can be close even when the two share few words, which the token-overlap rule never sees. The finding names the comment and the line it restates.",
    examples: &["// Increase the retry count by one\nattempts += 1;"],
    false_positives: "Comments that give a reason, a constraint, a link, or a number are not compared, nor are documentation comments, markers such as TODO, and comments above blocks. Short comments over terse code can still be close without being redundant.",
    replacements: &[],
};

pub static POLY004: RuleMetadata = RuleMetadata {
    id: "POLY004",
    module: Module::Polygraph,
    description: "Near-duplicate paragraph in another file",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "This paragraph nearly repeats a paragraph in another file.",
    suggestion: "Link to one canonical copy, or rewrite the paragraphs so each says something the other does not.",
    rationale: "Documentation that is copied between files drifts apart and pads the project. A static embedding finds paragraphs whose wording differs slightly but whose content is the same, which an exact-duplicate check misses. The finding names the file and line of the earlier twin.",
    examples: &[
        "The same installation paragraph in README.md and docs/install.md, with a few words changed.",
    ],
    false_positives: "Licenses, notices, and templates repeat on purpose and are skipped, as are list entries, table rows, release notes, and paragraphs in languages other than English. Intentional boilerplate, such as a standard security note, can be suppressed with a reason.",
    replacements: &[],
};

pub(super) fn metadata() -> Vec<&'static RuleMetadata> {
    vec![&POLY001, &POLY002, &POLY003, &POLY004]
}
