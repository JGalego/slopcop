//! Text preparation shared by the embedding rules.

use super::embed::Vector;
use super::model::Model;
use super::tokenizer::tokenize;

/// Embeds a sentence or paragraph of English prose. The model's vocabulary is English, so text in
/// another language or script breaks into a few shared fragments that look alike; such text, and
/// text made mostly of identifiers, has no vector. Markup tags are removed first.
pub(super) fn english_vector(model: &Model, text: &str) -> Option<Vector> {
    let plain = strip_tags(text);
    let letters = plain.chars().filter(|c| c.is_alphabetic()).count();
    let foreign = plain
        .chars()
        .filter(|c| c.is_alphabetic() && !c.is_ascii())
        .count();
    if letters == 0 || foreign * 20 > letters {
        return None;
    }
    let ids = tokenize(model, &plain);
    let fragments = ids
        .iter()
        .filter(|&&id| id == model.unknown() || model.is_continuation(id))
        .count();
    if ids.is_empty() || fragments * 4 > ids.len() {
        return None;
    }
    Vector::from_ids(model, &ids)
}

/// Replaces `<tag>` and `</tag>` markup, such as XML documentation comments, with spaces.
pub(super) fn strip_tags(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                plain.push(' ');
            }
            _ if !in_tag => plain.push(c),
            _ => {}
        }
    }
    if in_tag { text.to_owned() } else { plain }
}

/// Splits identifiers into lowercase words: `retryCount` and `retry_count` both give
/// `retry count`. Everything that is not a letter or digit separates words.
pub(super) fn identifier_words(code: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;
    for c in code.chars() {
        if !c.is_alphanumeric() {
            flush(&mut words, &mut current);
            previous = None;
            continue;
        }
        if c.is_uppercase() && previous.is_some_and(char::is_lowercase) {
            flush(&mut words, &mut current);
        }
        current.extend(c.to_lowercase());
        previous = Some(c);
    }
    flush(&mut words, &mut current);
    words
}

fn flush(words: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        words.push(std::mem::take(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_split_on_case_and_separators() {
        assert_eq!(
            identifier_words("retryCount += 1;"),
            ["retry", "count", "1"]
        );
        assert_eq!(
            identifier_words("max_retry_count"),
            ["max", "retry", "count"]
        );
        assert_eq!(
            identifier_words("self.cache.clear()"),
            ["self", "cache", "clear"]
        );
    }

    #[test]
    fn tags_are_removed_but_unclosed_brackets_are_kept() {
        assert_eq!(
            strip_tags("a <b>c</b> d")
                .split_whitespace()
                .collect::<Vec<_>>(),
            ["a", "c", "d"]
        );
        assert_eq!(strip_tags("if a < b then"), "if a < b then");
    }
}
