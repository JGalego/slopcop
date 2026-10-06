mod deadweight;
mod deadweight_extra;
pub(crate) mod papertrail;
mod vibecheck;
mod vibecheck_extra;

use std::borrow::Cow;
use std::cell::OnceCell;
use std::path::Path;

use crate::analysis::{
    Span, code_view, paragraph_spans, prose_view, restructured_text_prose, sentence_spans,
    word_count,
};
use crate::language::{Language, SourceType};
use crate::model::{Finding, Location, RuleMetadata};

pub trait Rule: Send + Sync {
    fn metadata(&self) -> &'static RuleMetadata;
    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>);
}

pub struct ScanContext<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub source_type: SourceType,
    prose: Cow<'a, str>,
    code: Cow<'a, str>,
    lower_source: OnceCell<String>,
    lower_prose: OnceCell<String>,
    prose_word_count: OnceCell<usize>,
    sentences: OnceCell<Vec<Span>>,
    paragraphs: OnceCell<Vec<Span>>,
}

impl ScanContext<'_> {
    #[must_use]
    pub fn new<'a>(path: &'a Path, source: &'a str, source_type: SourceType) -> ScanContext<'a> {
        ScanContext {
            path,
            source,
            source_type,
            prose: if source_type == SourceType::Documentation && is_restructured_text(path) {
                Cow::Owned(restructured_text_prose(source))
            } else {
                prose_view(source, source_type)
            },
            code: code_view(source, source_type),
            lower_source: OnceCell::new(),
            lower_prose: OnceCell::new(),
            prose_word_count: OnceCell::new(),
            sentences: OnceCell::new(),
            paragraphs: OnceCell::new(),
        }
    }

    #[must_use]
    pub fn prose(&self) -> &str {
        &self.prose
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn lower_source(&self) -> &str {
        self.lower_source
            .get_or_init(|| self.source.to_ascii_lowercase())
    }

    #[must_use]
    pub fn lower_prose(&self) -> &str {
        self.lower_prose
            .get_or_init(|| self.prose.to_ascii_lowercase())
    }

    #[must_use]
    pub fn prose_word_count(&self) -> usize {
        *self
            .prose_word_count
            .get_or_init(|| word_count(self.prose()))
    }

    #[must_use]
    pub fn sentences(&self) -> &[Span] {
        self.sentences.get_or_init(|| sentence_spans(self.prose()))
    }

    #[must_use]
    pub fn paragraphs(&self) -> &[Span] {
        self.paragraphs
            .get_or_init(|| paragraph_spans(self.prose()))
    }

    #[must_use]
    pub fn location(&self, offset: usize) -> Location {
        Location::at(self.source, offset)
    }

    #[must_use]
    pub fn line_at(&self, offset: usize) -> String {
        let start = self.source[..offset.min(self.source.len())]
            .rfind('\n')
            .map_or(0, |position| position + 1);
        let end = self.source[start..]
            .find('\n')
            .map_or(self.source.len(), |position| start + position);
        self.source[start..end].trim().to_owned()
    }
}

fn is_restructured_text(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rst"))
}

pub(super) type CheckFn = fn(&ScanContext<'_>, &'static RuleMetadata, &mut Vec<Finding>);

pub(super) struct BuiltinRule {
    pub metadata: &'static RuleMetadata,
    pub check_fn: CheckFn,
}

impl Rule for BuiltinRule {
    fn metadata(&self) -> &'static RuleMetadata {
        self.metadata
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        (self.check_fn)(context, self.metadata, findings);
    }
}

pub(super) fn emit(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<Finding>,
    offset: usize,
    observation: impl Into<Option<String>>,
) {
    findings.push(Finding {
        path: context.path.to_path_buf(),
        location: context.location(offset),
        rule_id: metadata.id,
        module: metadata.module,
        severity: metadata.default_severity,
        confidence: metadata.default_confidence,
        message: metadata.message.to_owned(),
        evidence: Some(context.line_at(offset)),
        observation: observation.into(),
        suggestion: metadata.suggestion,
    });
}

/// Reports whether a matched span carries an explanatory comment. Comment syntax follows the code
/// view: `#` in Python, Ruby, and shell, and `//` or `/* */` in every other language.
pub(super) fn matched_source_has_explanatory_comment(
    context: &ScanContext<'_>,
    start: usize,
    end: usize,
) -> bool {
    let source = &context.source[start..end];
    match context.source_type {
        SourceType::Code(Language::Python | Language::Ruby | Language::Shell) => source
            .lines()
            .filter_map(|line| line.split_once('#').map(|(_, comment)| comment))
            .any(is_explanatory_comment),
        SourceType::Code(_) => {
            source
                .lines()
                .filter_map(|line| line.split_once("//").map(|(_, comment)| comment))
                .any(is_explanatory_comment)
                || source.split("/*").skip(1).any(|comment| {
                    comment
                        .split_once("*/")
                        .is_some_and(|(comment, _)| is_explanatory_comment(comment))
                })
        }
        _ => false,
    }
}

fn is_explanatory_comment(comment: &str) -> bool {
    let comment = comment.trim().to_ascii_lowercase();
    !["todo", "fixme", "hack", "xxx"]
        .iter()
        .any(|marker| comment.starts_with(marker))
        && word_count(&comment) >= 3
}

#[must_use]
pub fn registry() -> Vec<Box<dyn Rule>> {
    let mut rules: Vec<Box<dyn Rule>> = vec![
        Box::new(deadweight::EmptyExceptionHandler),
        Box::new(vibecheck::FormulaicTransitionDensity),
    ];
    rules.extend(deadweight_extra::rules());
    rules.extend(vibecheck_extra::rules());
    rules.sort_by_key(|rule| rule.metadata().id);
    rules
}

#[must_use]
pub fn metadata_registry() -> Vec<&'static RuleMetadata> {
    let mut metadata: Vec<_> = registry().into_iter().map(|rule| rule.metadata()).collect();
    metadata.extend(
        papertrail::registry()
            .into_iter()
            .map(|rule| rule.metadata()),
    );
    metadata.sort_by_key(|metadata| metadata.id);
    metadata
}
