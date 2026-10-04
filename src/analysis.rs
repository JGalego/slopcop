use std::borrow::Cow;

use crate::language::{Language, SourceType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

#[must_use]
pub fn word_count(source: &str) -> usize {
    source
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .count()
}

#[must_use]
pub fn words(source: &str) -> Vec<String> {
    source
        .split(|character: char| !character.is_alphanumeric() && character != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub(crate) fn preceding_text(source: &str, end: usize, length: usize) -> &str {
    let mut start = end.saturating_sub(length);
    while !source.is_char_boundary(start) {
        start += 1;
    }
    &source[start..end]
}

pub(crate) fn python_block_ends_at(source: &str, start: usize, end: usize) -> bool {
    let indentation = |line: &str| {
        line.bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .map(|byte| if byte == b'\t' { 8 } else { 1 })
            .sum::<usize>()
    };
    let header_indent = indentation(source[start..].lines().next().unwrap_or(""));
    source[end..]
        .lines()
        .find(|line| !line.trim().is_empty())
        .is_none_or(|line| indentation(line) <= header_indent)
}

#[must_use]
pub fn sentence_spans(source: &str) -> Vec<Span> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let boundary = matches!(bytes[index], b'.' | b'!' | b'?')
            && bytes.get(index + 1).is_none_or(u8::is_ascii_whitespace);
        if boundary {
            push_trimmed_span(source, start, index + 1, &mut spans);
            start = index + 1;
        }
        index += 1;
    }
    push_trimmed_span(source, start, source.len(), &mut spans);
    spans
}

#[must_use]
pub fn paragraph_spans(source: &str) -> Vec<Span> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut start = 0;
    let mut index = 0;

    while index + 1 < bytes.len() {
        if bytes[index] == b'\n' {
            let mut end = index + 1;
            while end < bytes.len() && matches!(bytes[end], b'\n' | b'\r' | b' ' | b'\t') {
                end += 1;
            }
            if bytes[index + 1..end].contains(&b'\n') {
                push_trimmed_span(source, start, index, &mut spans);
                start = end;
                index = end;
                continue;
            }
        }
        index += 1;
    }
    push_trimmed_span(source, start, source.len(), &mut spans);
    spans
}

fn push_trimmed_span(source: &str, start: usize, end: usize, spans: &mut Vec<Span>) {
    let candidate = &source[start..end];
    let leading = candidate.len() - candidate.trim_start().len();
    let trailing = candidate.len() - candidate.trim_end().len();
    let trimmed_start = start + leading;
    let trimmed_end = end.saturating_sub(trailing);
    if trimmed_start < trimmed_end {
        spans.push(Span {
            start: trimmed_start,
            end: trimmed_end,
        });
    }
}

#[must_use]
pub fn prose_view(source: &str, source_type: SourceType) -> Cow<'_, str> {
    match source_type {
        SourceType::Documentation => Cow::Owned(mask_markdown_code(source)),
        SourceType::Text => Cow::Borrowed(source),
        SourceType::Code(language) => Cow::Owned(extract_code_prose(source, language)),
        SourceType::Configuration | SourceType::Unknown => Cow::Owned(blank(source)),
    }
}

#[must_use]
pub fn code_view(source: &str, source_type: SourceType) -> Cow<'_, str> {
    let SourceType::Code(language) = source_type else {
        return Cow::Owned(blank(source));
    };
    Cow::Owned(extract_code(source, language))
}

fn blank(source: &str) -> String {
    let bytes = source
        .bytes()
        .map(|byte| if byte == b'\n' { b'\n' } else { b' ' })
        .collect();
    String::from_utf8(bytes).expect("ASCII mask must be valid UTF-8")
}

fn mask_markdown_code(source: &str) -> String {
    let mut output = source.as_bytes().to_vec();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;

    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let delimiter = markdown_fence(line);
        if let Some((marker, length)) = fence {
            mask_range(&mut output, offset, offset + line.len());
            if delimiter.is_some_and(|(candidate, count)| {
                candidate == marker && count >= length && trimmed[count..].trim().is_empty()
            }) {
                fence = None;
            }
        } else if let Some(delimiter) = delimiter {
            mask_range(&mut output, offset, offset + line.len());
            fence = Some(delimiter);
        } else {
            mask_inline_code(&mut output, line, offset);
        }
        offset += line.len();
    }

    String::from_utf8(output).expect("masking preserves valid UTF-8")
}

fn markdown_fence(line: &str) -> Option<(u8, usize)> {
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 {
        return None;
    }
    let trimmed = &line[indentation..];
    let marker = *trimmed.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let count = trimmed.bytes().take_while(|byte| *byte == marker).count();
    (count >= 3).then_some((marker, count))
}

fn mask_inline_code(output: &mut [u8], line: &str, line_offset: usize) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let delimiter_start = index;
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        let delimiter_len = index - delimiter_start;
        let Some(relative_end) = find_repeated(&bytes[index..], b'`', delimiter_len) else {
            break;
        };
        let end = index + relative_end + delimiter_len;
        mask_range(output, line_offset + delimiter_start, line_offset + end);
        index = end;
    }
}

fn find_repeated(bytes: &[u8], needle: u8, count: usize) -> Option<usize> {
    bytes
        .windows(count)
        .position(|window| window.iter().all(|byte| *byte == needle))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ExtractionMode {
    Code,
    Prose,
}

fn extract_code_prose(source: &str, language: Language) -> String {
    extract_code_view(source, language, ExtractionMode::Prose)
}

fn extract_code(source: &str, language: Language) -> String {
    extract_code_view(source, language, ExtractionMode::Code)
}

fn extract_code_view(source: &str, language: Language, mode: ExtractionMode) -> String {
    let bytes = source.as_bytes();
    let mut output = match mode {
        ExtractionMode::Code => bytes.to_vec(),
        ExtractionMode::Prose => blank(source).into_bytes(),
    };
    let line_marker = match language {
        Language::Python | Language::Ruby | Language::Shell => Some(b'#'),
        _ => None,
    };
    let slash_comments = !matches!(
        language,
        Language::Python | Language::Ruby | Language::Shell
    );
    let block_comments = slash_comments;
    let mut index = 0;

    while index < bytes.len() {
        if language == Language::Rust {
            if let Some(end) = rust_raw_string_end(bytes, index) {
                mask_range(&mut output, index, end);
                index = end;
                continue;
            }
            if bytes[index] == b'\'' {
                if let Some(end) = rust_lifetime_end(source, index) {
                    index = end;
                    continue;
                }
            }
        }
        if language == Language::Python && starts_triple_quote(bytes, index) {
            let quote = bytes[index];
            let end = find_triple_end(bytes, index + 3, quote).unwrap_or(bytes.len());
            transform_range(
                &mut output,
                bytes,
                index,
                end,
                mode == ExtractionMode::Prose && starts_docstring(bytes, index),
            );
            index = end;
            continue;
        }
        if line_marker.is_some_and(|marker| bytes[index] == marker) {
            let end = find_line_end(bytes, index);
            transform_range(
                &mut output,
                bytes,
                index,
                end,
                mode == ExtractionMode::Prose,
            );
            index = end;
            continue;
        }
        if slash_comments && bytes[index..].starts_with(b"//") {
            let end = find_line_end(bytes, index);
            transform_range(
                &mut output,
                bytes,
                index,
                end,
                mode == ExtractionMode::Prose,
            );
            index = end;
            continue;
        }
        if block_comments && bytes[index..].starts_with(b"/*") {
            let end = find_bytes(bytes, index + 2, b"*/").map_or(bytes.len(), |end| end + 2);
            transform_range(
                &mut output,
                bytes,
                index,
                end,
                mode == ExtractionMode::Prose,
            );
            index = end;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            let end = skip_quoted(bytes, index, bytes[index]);
            mask_range(&mut output, index, end);
            index = end;
            continue;
        }
        index += 1;
    }

    String::from_utf8(output).expect("extraction preserves valid UTF-8")
}

fn rust_raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        return None;
    }
    let prefix = if bytes[start..].starts_with(b"br") {
        2
    } else if bytes[start] == b'r' {
        1
    } else {
        return None;
    };
    let mut quote = start + prefix;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let mut delimiter = vec![b'"'];
    delimiter.extend(std::iter::repeat_n(b'#', quote - start - prefix));
    Some(find_bytes(bytes, quote + 1, &delimiter).map_or(bytes.len(), |end| end + delimiter.len()))
}

fn rust_lifetime_end(source: &str, start: usize) -> Option<usize> {
    let rest = &source[start + 1..];
    if !rest
        .chars()
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_')
    {
        return None;
    }
    let length = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .map(char::len_utf8)
        .sum::<usize>();
    let end = start + 1 + length;
    (source.as_bytes().get(end) != Some(&b'\'')).then_some(end)
}

fn find_line_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |position| start + position)
}

fn transform_range(output: &mut [u8], source: &[u8], start: usize, end: usize, keep: bool) {
    if keep {
        copy_range(output, source, start, end);
    } else {
        mask_range(output, start, end);
    }
}

fn starts_docstring(bytes: &[u8], index: usize) -> bool {
    if !starts_triple_quote(bytes, index) {
        return false;
    }
    let line_start = bytes[..index]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |position| position + 1);
    bytes[line_start..index].iter().all(u8::is_ascii_whitespace)
}

fn starts_triple_quote(bytes: &[u8], index: usize) -> bool {
    index + 3 <= bytes.len()
        && matches!(bytes[index], b'\'' | b'"')
        && bytes[index..index + 3] == [bytes[index]; 3]
}

fn find_triple_end(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    bytes[start..]
        .windows(3)
        .position(|window| window == [quote; 3])
        .map(|position| start + position + 3)
}

fn find_bytes(bytes: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    bytes[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| start + position)
}

fn skip_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn mask_range(output: &mut [u8], start: usize, end: usize) {
    for byte in &mut output[start..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn copy_range(output: &mut [u8], source: &[u8], start: usize, end: usize) {
    output[start..end].copy_from_slice(&source[start..end]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_markdown_code_without_changing_offsets() {
        let source =
            "Text `that said` here.\n```text\nUltimately hidden.\n```\nUltimately visible.\n";
        let prose = prose_view(source, SourceType::Documentation);

        assert_eq!(prose.len(), source.len());
        assert!(!prose.contains("that said"));
        assert!(!prose.contains("hidden"));
        assert!(prose.contains("Ultimately visible"));
    }

    #[test]
    fn preserves_rust_lifetimes_and_masks_raw_strings() {
        let source = "fn run() -> &'static str { todo!() }\nlet text = r###\"quoted \" // TODO hidden\"###;\nlet byte = br#\"/* hidden */\"#;\nlet letter = 'é';\n// TODO visible\n";
        let kind = SourceType::Code(Language::Rust);
        let code = code_view(source, kind);
        let prose = prose_view(source, kind);
        assert!(code.contains("&'static str { todo!() }"));
        assert!(!code.contains("hidden"));
        assert!(!prose.contains("hidden"));
        assert!(prose.contains("TODO visible"));
        assert_eq!(code.len(), source.len());
        assert_eq!(prose.len(), source.len());
    }

    #[test]
    fn markdown_fences_require_matching_marker_and_length() {
        let source = "````text\n```\nHidden prose.\n~~~\nStill hidden.\n```` trailing\nStill hidden too.\n````\nVisible prose.\n";
        let prose = prose_view(source, SourceType::Documentation);
        assert!(!prose.contains("hidden"));
        assert!(prose.contains("Visible prose"));
        assert_eq!(prose.len(), source.len());
        assert_eq!(code_view(source, SourceType::Documentation).trim(), "");
    }

    #[test]
    fn extracts_comments_but_not_string_literals() {
        let source = "const text = \"Ultimately hidden\";\n// Ultimately visible\nrun();\n";
        let prose = prose_view(source, SourceType::Code(Language::TypeScript));

        assert_eq!(prose.len(), source.len());
        assert!(!prose.contains("hidden"));
        assert!(prose.contains("Ultimately visible"));
    }

    #[test]
    fn extracts_code_but_not_comments_or_literals() {
        let source = "const text = \"todo!()\";\n// todo!()\ntodo!();\n";
        let code = code_view(source, SourceType::Code(Language::Rust));

        assert_eq!(code.len(), source.len());
        assert_eq!(code.matches("todo!").count(), 1);
    }

    #[test]
    fn segments_sentences_and_paragraphs_with_source_offsets() {
        let source = "First sentence. Second sentence!\n\nThird paragraph?";
        let sentences = sentence_spans(source);
        let paragraphs = paragraph_spans(source);

        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[1].text(source), "Second sentence!");
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[1].text(source), "Third paragraph?");
    }
}
