//! A pure-Rust reimplementation of the BERT tokenizer that the model was trained with: clean the
//! text, space out CJK ideographs, strip accents, lowercase, split on whitespace and punctuation,
//! then match `WordPiece` greedily. `tests/polygraph_tokenizer.rs` checks it against the reference
//! implementation.

use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

use super::model::Model;

const MAX_WORD_CHARS: usize = 100;
const CONTINUATION: &str = "##";

/// Returns the token ids of `text`, with the unknown id for words the vocabulary cannot spell.
#[must_use]
pub fn tokenize(model: &Model, text: &str) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut rest = text;
    // Like the reference, a literal special token such as `[SEP]` is one token, found before
    // any normalization.
    while let Some((position, token, id)) = model
        .special_tokens()
        .iter()
        .filter_map(|(token, id)| rest.find(&**token).map(|position| (position, token, *id)))
        .min_by_key(|&(position, _, id)| (position, id))
    {
        plain_tokens(model, &rest[..position], &mut ids);
        ids.push(id);
        rest = &rest[position + token.len()..];
    }
    plain_tokens(model, rest, &mut ids);
    ids
}

fn plain_tokens(model: &Model, text: &str, ids: &mut Vec<u32>) {
    let normalized = normalize(text);
    for word in pre_tokenize(&normalized) {
        word_pieces(model, word, ids);
    }
}

fn normalize(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\0' || c == '\u{fffd}' || is_control(c) {
            continue;
        }
        let c = if c.is_whitespace() { ' ' } else { c };
        if is_cjk(c) {
            cleaned.push(' ');
            cleaned.push(c);
            cleaned.push(' ');
        } else {
            cleaned.push(c);
        }
    }
    cleaned
        .nfd()
        .filter(|&c| get_general_category(c) != GeneralCategory::NonspacingMark)
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_control(c: char) -> bool {
    !matches!(c, '\t' | '\n' | '\r')
        && matches!(
            get_general_category(c),
            GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::Unassigned
                | GeneralCategory::PrivateUse
                | GeneralCategory::Surrogate
        )
}

fn is_cjk(c: char) -> bool {
    matches!(
        u32::from(c),
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x2_0000..=0x2_A6DF
            | 0x2_A700..=0x2_B73F
            | 0x2_B740..=0x2_B81F
            | 0x2_B820..=0x2_CEAF
            | 0xF900..=0xFAFF
            | 0x2_F800..=0x2_FA1F
    )
}

fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            get_general_category(c),
            GeneralCategory::ConnectorPunctuation
                | GeneralCategory::DashPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::OtherPunctuation
        )
}

/// Splits on whitespace, then makes every punctuation character its own word.
fn pre_tokenize(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    for chunk in text.split_whitespace() {
        let mut start = 0;
        for (index, c) in chunk.char_indices() {
            if is_punctuation(c) {
                if start < index {
                    words.push(&chunk[start..index]);
                }
                words.push(&chunk[index..index + c.len_utf8()]);
                start = index + c.len_utf8();
            }
        }
        if start < chunk.len() {
            words.push(&chunk[start..]);
        }
    }
    words
}

fn word_pieces(model: &Model, word: &str, ids: &mut Vec<u32>) {
    let boundaries: Vec<usize> = word
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(word.len()))
        .collect();
    if boundaries.len() - 1 > MAX_WORD_CHARS {
        ids.push(model.unknown());
        return;
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    while start < boundaries.len() - 1 {
        let mut end = boundaries.len() - 1;
        let mut found = None;
        while start < end {
            let slice = &word[boundaries[start]..boundaries[end]];
            let id = if start == 0 {
                model.id(slice)
            } else {
                model.id(&format!("{CONTINUATION}{slice}"))
            };
            if let Some(id) = id {
                found = Some(id);
                break;
            }
            end -= 1;
        }
        let Some(id) = found else {
            ids.push(model.unknown());
            return;
        };
        pieces.push(id);
        start = end;
    }
    ids.extend(pieces);
}
