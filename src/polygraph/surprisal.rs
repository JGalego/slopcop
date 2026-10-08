//! `POLY001` and `POLY002`: rules over how predictable a language model finds each sentence.

use std::sync::Arc;

use crate::language::SourceType;
use crate::model::{Finding, RuleMetadata};
use crate::rules::polygraph::{POLY001, POLY002};
use crate::rules::vibecheck_extra::{
    in_list_paragraph, is_prose, is_release_notes, is_running_text, paragraph_index,
};
use crate::rules::{Rule, ScanContext, emit};

use super::lm::{LanguageModel, installed_language_model};

/// A document needs this many prose words to be scored, so short files are never judged.
const MIN_DOCUMENT_WORDS: usize = 200;
/// At most this many tokens of a document are scored, which bounds the cost of a long file.
const MAX_TOKENS: usize = 1_024;
/// A sentence needs this many scored tokens to count.
const MIN_SENTENCE_TOKENS: usize = 8;
/// `POLY001`: this many consecutive sentences, each averaging at most `RUN_BITS`, make a run.
const RUN_LENGTH: usize = 5;
const RUN_BITS: f32 = 3.0;
/// `POLY002`: this many scored sentences are needed, and a spread of at most `FLAT_SPREAD` bits.
const FLAT_MIN_SENTENCES: usize = 20;
const FLAT_SPREAD: f32 = 1.0;

/// The mean surprisal of one sentence, in bits per token.
#[derive(Clone, Debug)]
pub struct SentenceScore {
    offset: usize,
    bits: f32,
}

fn thresholds() -> (f32, f32) {
    let read = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
    }; // CALIBRATION-ONLY
    (
        read("POLY_T1").unwrap_or(RUN_BITS),
        read("POLY_T2").unwrap_or(FLAT_SPREAD),
    )
}

fn scores<'a>(
    context: &'a ScanContext<'_>,
    lm: &Arc<LanguageModel>,
) -> Option<&'a [SentenceScore]> {
    context.sentence_scores(|| compute(context, lm))
}

fn compute(context: &ScanContext<'_>, lm: &LanguageModel) -> Option<Vec<SentenceScore>> {
    if !is_prose(context)
        || !is_running_text(context)
        || is_release_notes(context)
        || !matches!(
            context.source_type,
            SourceType::Documentation | SourceType::Text
        )
        || context.prose_word_count() < MIN_DOCUMENT_WORDS
    {
        return None;
    }
    let prose = context.prose();
    // Rebuild the running text without list entries, so that the model reads sentences and not
    // bullets, and remember where each sentence landed.
    let mut assembled = String::new();
    let mut placed: Vec<(usize, usize, usize)> = Vec::new();
    let mut previous_paragraph = None;
    for sentence in context.sentences() {
        let paragraph = paragraph_index(context, sentence.start)?;
        if in_list_paragraph(context, sentence.start) {
            continue;
        }
        if previous_paragraph.is_some_and(|previous| previous != paragraph) {
            assembled.push_str("\n\n");
        } else if previous_paragraph.is_some() {
            assembled.push(' ');
        }
        previous_paragraph = Some(paragraph);
        let start = assembled.len();
        let mut words = sentence.text(prose).split_whitespace();
        if let Some(first) = words.next() {
            assembled.push_str(first);
            for word in words {
                assembled.push(' ');
                assembled.push_str(word);
            }
        }
        placed.push((start, assembled.len(), sentence.start));
    }
    let tokens = lm.score(&assembled, MAX_TOKENS).ok()?;
    let mut result = Vec::new();
    for (start, end, offset) in placed {
        let own: Vec<f32> = tokens
            .iter()
            .filter(|token| token.start >= start && token.start < end && !token.first_in_window)
            .map(|token| token.bits)
            .collect();
        if own.len() >= MIN_SENTENCE_TOKENS {
            #[allow(clippy::cast_precision_loss)]
            let bits = own.iter().sum::<f32>() / own.len() as f32;
            result.push(SentenceScore { offset, bits });
        }
    }
    if let Ok(path) = std::env::var("POLY_LM_DUMP") {
        // CALIBRATION-ONLY
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let bits: Vec<String> = result
                .iter()
                .map(|score| format!("{:.3}", score.bits))
                .collect();
            let _ = writeln!(file, "{}\t{}", context.path.display(), bits.join(" "));
        }
    }
    Some(result)
}

fn model_label() -> &'static str {
    "SmolLM2-135M"
}

pub(super) struct PredictableRun;

impl Rule for PredictableRun {
    fn metadata(&self) -> &'static RuleMetadata {
        &POLY001
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        let Some(lm) = installed_language_model() else {
            return;
        };
        let Some(scored) = scores(context, lm) else {
            return;
        };
        let (limit, _) = thresholds();
        let mut index = 0;
        while index < scored.len() {
            let length = scored[index..]
                .iter()
                .take_while(|score| score.bits <= limit)
                .count();
            if length >= RUN_LENGTH {
                let run = &scored[index..index + length];
                #[allow(clippy::cast_precision_loss)]
                let mean = run.iter().map(|score| score.bits).sum::<f32>() / length as f32;
                emit(
                    context,
                    &POLY001,
                    findings,
                    run[0].offset,
                    Some(format!(
                        "{length} consecutive sentences from line {} to line {} average {mean:.2} bits per token ({})",
                        context.location(run[0].offset).line,
                        context.location(run[length - 1].offset).line,
                        model_label(),
                    )),
                );
            }
            index += length.max(1);
        }
    }
}

pub(super) struct FlatSurprisal;

impl Rule for FlatSurprisal {
    fn metadata(&self) -> &'static RuleMetadata {
        &POLY002
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        let Some(lm) = installed_language_model() else {
            return;
        };
        let Some(scored) = scores(context, lm) else {
            return;
        };
        if scored.len() < FLAT_MIN_SENTENCES {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let count = scored.len() as f32;
        let mean = scored.iter().map(|score| score.bits).sum::<f32>() / count;
        let spread = (scored
            .iter()
            .map(|score| (score.bits - mean).powi(2))
            .sum::<f32>()
            / count)
            .sqrt();
        let (_, limit) = thresholds();
        if spread > limit {
            return;
        }
        let lowest = scored
            .iter()
            .min_by(|a, b| a.bits.total_cmp(&b.bits))
            .expect("non-empty");
        let highest = scored
            .iter()
            .max_by(|a, b| a.bits.total_cmp(&b.bits))
            .expect("non-empty");
        emit(
            context,
            &POLY002,
            findings,
            scored[0].offset,
            Some(format!(
                "{} sentences average {mean:.2} bits per token with a spread of {spread:.2}; lowest {:.2} on line {}, highest {:.2} on line {} ({})",
                scored.len(),
                lowest.bits,
                context.location(lowest.offset).line,
                highest.bits,
                context.location(highest.offset).line,
                model_label(),
            )),
        );
    }
}
