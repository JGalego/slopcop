//! `POLY003`: a line comment that restates the code line below it.

use crate::language::{Language, SourceType};
use crate::model::{Finding, RuleMetadata};
use crate::rules::polygraph::POLY003;
use crate::rules::vibecheck_extra::quotes_specification;
use crate::rules::{Rule, ScanContext, emit};

use super::embed::{Vector, at_least};
use super::installed_model;
use super::prep::identifier_words;
use super::tokenize;

/// A comment and its code are a restatement at this cosine or above: `NUMERATOR / DENOMINATOR`.
const THRESHOLD: (u32, u32) = (9, 10);
/// A comment needs this many words, and at most this many, to be compared.
const COMMENT_WORDS: std::ops::RangeInclusive<usize> = 3..=14;
/// The code line needs this many identifier words.
const MIN_CODE_WORDS: usize = 2;
/// Words that mark a comment as giving a reason or a constraint rather than restating.
const RATIONALE: &[&str] = &[
    "because",
    "since",
    "so that",
    "otherwise",
    "workaround",
    "must",
    "unless",
    "avoid",
    "note",
    "todo",
    "fixme",
    "hack",
    "xxx",
    "safety",
    "warning",
    "careful",
    "required",
    "why",
    "should",
    "http",
    "never",
    "always",
    "ensure",
    "only",
    "until",
    "before",
    "after",
    "when",
    "if ",
];

pub(super) struct CommentRestatesCode;

impl Rule for CommentRestatesCode {
    fn metadata(&self) -> &'static RuleMetadata {
        &POLY003
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        let SourceType::Code(language) = context.source_type else {
            return;
        };
        let Some(model) = installed_model() else {
            return;
        };
        // Comments that quote a specification's algorithm describe its steps, not the code.
        if quotes_specification(context) {
            return;
        }
        let marker = marker(language);
        let lines: Vec<(usize, &str)> = line_offsets(context.source).collect();
        let mut index = 0;
        while index < lines.len() {
            if comment_text(lines[index].1, marker).is_none() {
                index += 1;
                continue;
            }
            let start = index;
            while index < lines.len() && comment_text(lines[index].1, marker).is_some() {
                index += 1;
            }
            // Only a short block directly above a code line is compared.
            let block = &lines[start..index];
            let Some(&(code_offset, code)) = lines.get(index) else {
                break;
            };
            if block.len() > 2 {
                continue;
            }
            let text = block
                .iter()
                .filter_map(|(_, line)| comment_text(line, marker))
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(cosine) = restates(model, &text, code) {
                emit(
                    context,
                    &POLY003,
                    findings,
                    block[0].0,
                    Some(format!(
                        "comment is {cosine} thousandths cosine to the code on line {}",
                        context.location(code_offset).line
                    )),
                );
            }
        }
    }
}

fn restates(model: &super::Model, comment: &str, code: &str) -> Option<i128> {
    let lowered = comment.to_ascii_lowercase();
    let words = comment.split_whitespace().count();
    if !COMMENT_WORDS.contains(&words)
        || RATIONALE.iter().any(|marker| lowered.contains(marker))
        || comment
            .chars()
            .any(|c| c.is_ascii_digit() || matches!(c, '`' | '/' | '_' | '(' | ';'))
    {
        return None;
    }
    let code_words = identifier_words(code);
    if code_words.len() < MIN_CODE_WORDS || is_structural(code) {
        return None;
    }
    let comment_vector = Vector::from_ids_min(model, &tokenize(model, comment), 3)?;
    let code_vector = Vector::from_ids_min(model, &tokenize(model, &code_words.join(" ")), 2)?;
    at_least(&comment_vector, &code_vector, THRESHOLD.0, THRESHOLD.1)
        .then(|| comment_vector.cosine_milli(&code_vector))
}

/// Closing braces, `else`, and other lines that carry no statement of their own.
fn is_structural(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty()
        || trimmed.starts_with(['}', ')', ']', '{'])
        || trimmed.starts_with("else")
        || trimmed.starts_with('#')
}

fn marker(language: Language) -> &'static str {
    match language {
        Language::Python | Language::Ruby | Language::Shell => "#",
        _ => "//",
    }
}

/// The text of a comment-only line, or `None` for code, blank lines, doc comments, and shebangs.
fn comment_text<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = line.trim_start().strip_prefix(marker)?;
    if rest.starts_with(['/', '!']) || (marker == "#" && rest.starts_with('#')) {
        return None;
    }
    let text = rest.trim();
    (!text.is_empty()).then_some(text)
}

fn line_offsets(source: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    source.split_inclusive('\n').map(move |line| {
        let start = offset;
        offset += line.len();
        (start, line.trim_end_matches(['\n', '\r']))
    })
}
