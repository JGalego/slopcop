//! `POLY004`: a paragraph that nearly repeats a paragraph in another file. Each file contributes
//! its paragraphs while it is scanned; one pass over all of them finds the twins.
//!
//! Candidates come from locality-sensitive hashing with fixed integer hyperplanes, and every
//! candidate is then checked with the exact integer cosine, so hashing only decides which pairs
//! are looked at and never what is reported.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::{Finding, Location};
use crate::rules::vibecheck_extra::{
    in_list_paragraph, is_prose, is_release_notes, is_running_text, paragraph_index,
};
use crate::rules::{ScanContext, polygraph::POLY004};
use crate::suppression::suppressed_lines;

use super::embed::{Vector, at_least};
use super::model::{MODEL_SHA256, Model, short_hash};
use super::prep::english_vector;

/// Paragraphs closer than this cosine are twins: `NUMERATOR / DENOMINATOR`.
const THRESHOLD: (u32, u32) = (97, 100);
/// A paragraph needs this many words to be compared.
const MIN_WORDS: usize = 20;
/// Two twins' word counts differ by at most this share of the longer one.
const MAX_LENGTH_GAP_PERCENT: usize = 40;
/// Hash bands, each of this many bits, in the candidate search.
const BANDS: usize = 8;
const BAND_BITS: usize = 12;
/// Up to this many paragraphs, every pair is compared and no hashing is needed.
const BRUTE_FORCE_LIMIT: usize = 1_500;
/// A bucket with more paragraphs than this is a template, not a duplicate.
const MAX_BUCKET: usize = 64;

/// One comparable paragraph of a scanned file.
#[derive(Clone, Debug)]
pub struct Paragraph {
    path: PathBuf,
    location: Location,
    evidence: String,
    words: usize,
    vector: Vector,
}

/// The comparable paragraphs of one file, or none when the file is not running English text.
#[must_use]
pub fn collect(context: &ScanContext<'_>, model: &Model) -> Vec<Paragraph> {
    if !is_prose(context)
        || !is_running_text(context)
        || is_release_notes(context)
        || is_notice_file(context)
    {
        return Vec::new();
    }
    let prose = context.prose();
    let suppressed = suppressed_lines(context.source, POLY004.id);
    context
        .paragraphs()
        .iter()
        .filter_map(|span| {
            let text = span.text(prose);
            let (text, skipped) = without_headings(text);
            let words = text.split_whitespace().count();
            if words < MIN_WORDS || in_list_paragraph(context, span.start) || mostly_entries(text) {
                return None;
            }
            let start = span.start + skipped;
            paragraph_index(context, span.start)?;
            let location = context.location(start);
            if suppressed.contains(&location.line) {
                return None;
            }
            Some(Paragraph {
                path: context.path.to_path_buf(),
                location,
                evidence: context.line_at(start),
                words,
                vector: english_vector(model, text)?,
            })
        })
        .collect()
}

/// A paragraph's text without the Markdown heading lines that open it, and the bytes skipped.
fn without_headings(text: &str) -> (&str, usize) {
    let mut skipped = 0;
    for line in text.split_inclusive('\n') {
        if !line.trim_start().starts_with('#') {
            break;
        }
        skipped += line.len();
    }
    (&text[skipped..], skipped)
}

/// Whether most lines are list items or table rows, which repeat their template by design.
fn mostly_entries(text: &str) -> bool {
    let lines: Vec<_> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let entries = lines
        .iter()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with(['-', '*', '+', '|', '>'])
                || line.split_once(['.', ')']).is_some_and(|(number, _)| {
                    !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
                })
        })
        .count();
    entries * 2 > lines.len()
}

/// Licenses, notices, and contributor lists repeat by design, as do vendored copies of them.
fn is_notice_file(context: &ScanContext<'_>) -> bool {
    const NOTICE_NAMES: &[&str] = &[
        "license",
        "licence",
        "copying",
        "notice",
        "unlicense",
        "copyright",
        "authors",
        "patents",
        "third_party",
        "third-party",
        "thirdparty",
        "vendor",
    ];
    let in_notice_directory = context.path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        NOTICE_NAMES.iter().any(|notice| name.starts_with(notice))
    });
    if in_notice_directory {
        return true;
    }
    let stem = context
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    NOTICE_NAMES.iter().any(|name| stem.starts_with(name))
}

/// Finds every paragraph that has a twin in an earlier file, and reports it once.
#[must_use]
pub fn find(mut paragraphs: Vec<Paragraph>) -> Vec<Finding> {
    paragraphs.sort_by(|left, right| {
        (&left.path, left.location.line).cmp(&(&right.path, right.location.line))
    });
    let Some(first) = paragraphs.first() else {
        return Vec::new();
    };
    // For each paragraph, the earliest paragraph in another file that it nearly repeats.
    let mut twin: Vec<Option<usize>> = vec![None; paragraphs.len()];
    let mut consider = |earlier: usize, later: usize| {
        if twin[later].is_none_or(|current| earlier < current)
            && are_twins(&paragraphs[earlier], &paragraphs[later])
        {
            twin[later] = Some(earlier);
        }
    };
    if paragraphs.len() <= BRUTE_FORCE_LIMIT {
        for later in 1..paragraphs.len() {
            for earlier in 0..later {
                consider(earlier, later);
            }
        }
    } else {
        let dims = first.vector.len();
        let mean = mean_vector(&paragraphs, dims);
        let planes = hyperplanes(dims);
        let mut buckets: HashMap<(usize, u16), Vec<usize>> = HashMap::new();
        for (index, paragraph) in paragraphs.iter().enumerate() {
            let signature = signature(&paragraph.vector, &mean, &planes);
            for (band, key) in signature.into_iter().enumerate() {
                buckets.entry((band, key)).or_default().push(index);
            }
        }
        let mut keys: Vec<_> = buckets.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let members = &buckets[&key];
            if members.len() > MAX_BUCKET {
                continue;
            }
            for (position, &later) in members.iter().enumerate() {
                for &earlier in &members[..position] {
                    consider(earlier, later);
                }
            }
        }
    }

    twin.iter()
        .enumerate()
        .filter_map(|(later, earlier)| {
            let (paragraph, other) = (&paragraphs[later], &paragraphs[(*earlier)?]);
            Some(Finding {
                path: paragraph.path.clone(),
                location: paragraph.location.clone(),
                rule_id: POLY004.id,
                module: POLY004.module,
                severity: POLY004.default_severity,
                confidence: POLY004.default_confidence,
                message: POLY004.message.to_owned(),
                evidence: Some(paragraph.evidence.clone()),
                observation: Some(format!(
                    "paragraph is {} thousandths cosine to the one at {}:{} (model {})",
                    paragraph.vector.cosine_milli(&other.vector),
                    other.path.display(),
                    other.location.line,
                    short_hash(MODEL_SHA256),
                )),
                suggestion: POLY004.suggestion,
            })
        })
        .collect()
}

fn are_twins(left: &Paragraph, right: &Paragraph) -> bool {
    let (longer, shorter) = (left.words.max(right.words), left.words.min(right.words));
    left.path != right.path
        && (longer - shorter) * 100 <= longer * MAX_LENGTH_GAP_PERCENT
        && at_least(&left.vector, &right.vector, THRESHOLD.0, THRESHOLD.1)
}

/// The integer mean of all vectors. Hashing subtracts it so that the bits separate paragraphs
/// instead of all pointing the way the average text does.
fn mean_vector(paragraphs: &[Paragraph], dims: usize) -> Vec<i64> {
    let mut sums = vec![0_i64; dims];
    for paragraph in paragraphs {
        for (sum, value) in sums.iter_mut().zip(paragraph.vector.values()) {
            *sum += i64::from(*value);
        }
    }
    let count = i64::try_from(paragraphs.len()).expect("paragraph count fits i64");
    sums.iter().map(|sum| sum / count).collect()
}

/// `BANDS * BAND_BITS` random-sign hyperplanes from a fixed generator.
fn hyperplanes(dims: usize) -> Vec<Vec<i8>> {
    let mut state = 0x5C0B_C0B5_u64;
    let mut next = move || {
        // splitmix64
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    (0..BANDS * BAND_BITS)
        .map(|_| {
            (0..dims)
                .map(|_| if next() & 1 == 0 { 1 } else { -1 })
                .collect()
        })
        .collect()
}

fn signature(vector: &Vector, mean: &[i64], planes: &[Vec<i8>]) -> [u16; BANDS] {
    let centered: Vec<i64> = vector
        .values()
        .iter()
        .zip(mean)
        .map(|(&value, &mean)| i64::from(value) - mean)
        .collect();
    let mut bands = [0_u16; BANDS];
    for (index, plane) in planes.iter().enumerate() {
        let projection: i64 = plane
            .iter()
            .zip(&centered)
            .map(|(&sign, &value)| i64::from(sign) * value)
            .sum();
        if projection >= 0 {
            bands[index / BAND_BITS] |= 1 << (index % BAND_BITS);
        }
    }
    bands
}
