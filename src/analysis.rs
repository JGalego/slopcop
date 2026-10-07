use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

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
    let header_indent = indentation(source[start..].lines().next().unwrap_or(""));
    source[end..]
        .lines()
        .find(|line| !line.trim().is_empty())
        .is_none_or(|line| indentation(line) <= header_indent)
}

/// Splits prose into sentences. Sentences never cross a paragraph boundary, so a heading or a
/// comment without terminal punctuation does not merge with the text that follows it.
#[must_use]
pub fn sentence_spans(source: &str) -> Vec<Span> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    for paragraph in paragraph_spans(source) {
        let mut start = paragraph.start;
        for index in paragraph.start..paragraph.end {
            let boundary = matches!(bytes[index], b'.' | b'!' | b'?')
                && bytes.get(index + 1).is_none_or(u8::is_ascii_whitespace);
            if boundary {
                push_trimmed_span(source, start, index + 1, &mut spans);
                start = index + 1;
            }
        }
        push_trimmed_span(source, start, paragraph.end, &mut spans);
    }
    spans
}

/// Splits prose into runs of lines that contain words. Blank lines, masked code, and lines of
/// only punctuation, such as underlines, rules, or empty comment markers, separate paragraphs.
#[must_use]
pub fn paragraph_spans(source: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = None;
    let mut end = 0;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if line.chars().any(char::is_alphanumeric) {
            start.get_or_insert(offset);
            end = offset + line.len();
        } else if let Some(paragraph_start) = start.take() {
            push_trimmed_span(source, paragraph_start, end, &mut spans);
        }
        offset += line.len();
    }
    if let Some(paragraph_start) = start {
        push_trimmed_span(source, paragraph_start, end, &mut spans);
    }
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

/// Returns the prose of a reStructuredText document with directives, literal and doctest blocks,
/// roles, field markers, and inline literals masked. Offsets match `source`.
#[must_use]
pub(crate) fn restructured_text_prose(source: &str) -> String {
    let mut output = source.as_bytes().to_vec();
    mask_restructured_text(&mut output, source, 0);
    String::from_utf8(output).expect("masking preserves valid UTF-8")
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
    let mut front_matter = false;
    let mut html_comment = false;
    let mut first_line = true;
    let mut offset = 0;

    for line in source.split_inclusive('\n') {
        let trimmed_line = line.trim();
        if first_line && line.trim_start_matches('\u{feff}').trim().eq("---") {
            front_matter = true;
            mask_range(&mut output, offset, offset + line.len());
            first_line = false;
            offset += line.len();
            continue;
        }
        first_line = false;
        if front_matter {
            mask_range(&mut output, offset, offset + line.len());
            if matches!(trimmed_line, "---" | "...") {
                front_matter = false;
            }
            offset += line.len();
            continue;
        }
        if html_comment || line.contains("<!--") {
            mask_html_comments(&mut output, line, offset, &mut html_comment);
            offset += line.len();
            continue;
        }
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

fn mask_html_comments(output: &mut [u8], line: &str, line_offset: usize, in_comment: &mut bool) {
    let mut index = 0;
    while index < line.len() {
        if *in_comment {
            if let Some(relative_end) = line[index..].find("-->") {
                let end = index + relative_end + 3;
                mask_range(output, line_offset + index, line_offset + end);
                *in_comment = false;
                index = end;
            } else {
                mask_range(output, line_offset + index, line_offset + line.len());
                break;
            }
        } else if let Some(relative_start) = line[index..].find("<!--") {
            index += relative_start;
            *in_comment = true;
        } else {
            break;
        }
    }
}

fn markdown_fence(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start();
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

/// Directives whose indented body is prose rather than code, data, or configuration.
const PROSE_DIRECTIVES: &[&str] = &[
    "admonition",
    "attention",
    "caution",
    "container",
    "danger",
    "deprecated",
    "error",
    "hint",
    "important",
    "note",
    "only",
    "rubric",
    "seealso",
    "sidebar",
    "tip",
    "topic",
    "versionadded",
    "versionchanged",
    "versionremoved",
    "warning",
];

#[derive(Clone, Copy)]
enum RstBlock {
    /// Mask blank lines and lines indented past this column.
    Masked(usize),
    /// Mask option lines indented past this column, then keep the directive body.
    Options(usize),
    /// A paragraph ended with `::`; a following block indented past this column is literal.
    PendingLiteral(usize),
    /// Mask lines until the next blank line.
    Doctest,
}

fn mask_restructured_text(output: &mut [u8], source: &str, base: usize) {
    let mut block = None;
    let mut offset = base;
    for line in source.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim();
        let indent = indentation(line);
        match block {
            Some(RstBlock::Masked(column)) if trimmed.is_empty() || indent > column => {
                mask_range(output, start, offset);
                continue;
            }
            Some(RstBlock::Options(column)) if indent > column && trimmed.starts_with(':') => {
                mask_range(output, start, offset);
                continue;
            }
            Some(RstBlock::PendingLiteral(_)) if trimmed.is_empty() => continue,
            Some(RstBlock::PendingLiteral(column)) if indent > column => {
                mask_range(output, start, offset);
                block = Some(RstBlock::Masked(column));
                continue;
            }
            Some(RstBlock::Doctest) if !trimmed.is_empty() => {
                mask_range(output, start, offset);
                continue;
            }
            _ => block = None,
        }
        if trimmed == ".." || trimmed.starts_with(".. ") {
            mask_range(output, start, offset);
            let name = trimmed[2..]
                .trim_start()
                .split_once("::")
                .map_or("", |(name, _)| name.trim());
            block = Some(if PROSE_DIRECTIVES.contains(&name) {
                RstBlock::Options(indent)
            } else {
                RstBlock::Masked(indent)
            });
            continue;
        }
        if trimmed.starts_with(">>>") {
            mask_range(output, start, offset);
            block = Some(RstBlock::Doctest);
            continue;
        }
        mask_rst_inline(output, line, start);
        if let Some(prefix) = trimmed.strip_suffix("::") {
            let marker = start + line.trim_end().len() - 2;
            // `text::` renders as `text:`, while a detached `::` disappears entirely.
            if prefix.is_empty() || prefix.ends_with(char::is_whitespace) {
                mask_range(output, marker, marker + 2);
            } else {
                mask_range(output, marker + 1, marker + 2);
            }
            block = Some(RstBlock::PendingLiteral(indent));
        }
    }
}

fn mask_rst_inline(output: &mut [u8], line: &str, line_offset: usize) {
    for found in rst_role_matcher().find_iter(line) {
        mask_range(
            output,
            line_offset + found.start(),
            line_offset + found.end() - 1,
        );
    }
    if let Some(found) = rst_field_matcher().find(line) {
        mask_range(
            output,
            line_offset + found.start(),
            line_offset + found.end(),
        );
    }
    mask_inline_code(output, line, line_offset);
}

fn rst_role_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r":[A-Za-z][\w.+-]*(?::[A-Za-z][\w.+-]*)*:`")
            .expect("reStructuredText role regex must compile")
    })
}

fn rst_field_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"^[ \t]*:[^:\s`][^:`]*:(?:[ \t]|\r?$)")
            .expect("reStructuredText field regex must compile")
    })
}

/// Masks fenced Markdown examples inside Rust `///` and `//!` documentation comments.
fn mask_doc_comment_examples(output: &mut [u8], source: &str) {
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let content = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"));
        if let Some(content) = content {
            let delimiter = markdown_fence(content);
            if fence.is_some() || delimiter.is_some() {
                mask_range(output, offset, offset + line.len());
            }
            if let Some((marker, length)) = fence {
                if delimiter
                    .is_some_and(|(candidate, count)| candidate == marker && count >= length)
                {
                    fence = None;
                }
            } else {
                fence = delimiter;
            }
        } else {
            fence = None;
        }
        offset += line.len();
    }
}

/// Whether source links a web or Internet standard, such as a WHATWG, W3C, TC39, or IETF
/// specification. Browser engines and protocol libraries annotate such an implementation with the
/// algorithm it follows, step numbers and wording included, as in
/// `// Step 5.4.1: Let requestList be a list.`.
pub(crate) fn links_specification(source: &str) -> bool {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER
        .get_or_init(|| {
            // Community group drafts such as `webaudio.github.io/web-audio-api/#...` count when they
            // link a section anchor.
            Regex::new(
                r##"(?i)https?://(?:[a-z0-9-]+\.)*(?:spec\.whatwg\.org|w3c\.github\.io|wicg\.github\.io|w3\.org/TR/|drafts\.[a-z-]+\.org|tc39\.es|ietf\.org/|rfc-editor\.org)|https?://[a-z0-9-]+\.github\.io/[^\s>)"#]*#"##,
            )
            .expect("specification link regex must compile")
        })
        .is_match(source)
}

/// Returns the width of a line's leading whitespace, counting a tab as eight columns.
pub(crate) fn indentation(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .map(|byte| if byte == b'\t' { 8 } else { 1 })
        .sum()
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
            let docstring = mode == ExtractionMode::Prose && starts_docstring(bytes, index);
            transform_range(&mut output, bytes, index, end, docstring);
            if docstring {
                mask_restructured_text(&mut output, &source[index..end], index);
            }
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
    if language == Language::Rust && mode == ExtractionMode::Prose {
        mask_doc_comment_examples(&mut output, source);
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
    fn masks_markdown_front_matter_and_html_comments() {
        let source = "---\ntitle: Hidden metadata\nsummary: Ultimately hidden.\n---\nVisible prose.\n<!-- END MANUAL: hidden marker.\nStill hidden. -->\nStill visible.\n";
        let prose = prose_view(source, SourceType::Documentation);

        assert_eq!(prose.len(), source.len());
        assert!(!prose.contains("Hidden metadata"));
        assert!(!prose.contains("Ultimately hidden"));
        assert!(!prose.contains("END MANUAL"));
        assert!(!prose.contains("Still hidden"));
        assert!(prose.contains("Visible prose"));
        assert!(prose.contains("Still visible"));
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
    fn masks_indented_markdown_fences() {
        let source = "<details>\n\n    ```yaml\n    # Hidden configuration comment\n    value: hidden\n    ```\n\n</details>\nVisible prose.\n";
        let prose = prose_view(source, SourceType::Documentation);

        assert_eq!(prose.len(), source.len());
        assert!(!prose.contains("Hidden configuration comment"));
        assert!(!prose.contains("value: hidden"));
        assert!(prose.contains("Visible prose"));
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
    fn masks_restructured_text_markup_but_keeps_prose() {
        let source = ".. module:: flask\n\nUse :class:`~flask.Flask` for apps. See ``g`` here::\n\n    # Code is not prose.\n    app = Flask(__name__)\n\n.. code-block:: python\n    :emphasize-lines: 1\n\n    hidden_call()\n\n.. note::\n    :class: tip\n\n    Notes stay visible.\n\n>>> doctest_hidden()\nresult_hidden\n\n:param timeout: Seconds to wait.\n";
        let prose = restructured_text_prose(source);
        assert_eq!(prose.len(), source.len());
        for hidden in [
            "module",
            ":class:",
            "Flask",
            "``g``",
            "::",
            "Code is not",
            "hidden",
            "emphasize",
            "tip",
            ":param timeout:",
        ] {
            assert!(
                !prose.contains(hidden),
                "{hidden} should be masked in {prose:?}"
            );
        }
        for visible in [
            "Use",
            "for apps.",
            "See",
            "here:",
            "Notes stay visible.",
            "Seconds to wait.",
        ] {
            assert!(
                prose.contains(visible),
                "{visible} should stay in {prose:?}"
            );
        }
    }

    #[test]
    fn masks_examples_in_docstrings_and_rust_doc_comments() {
        let python = "def stream():\n    \"\"\"Stream a response.\n\n    .. code-block:: python\n\n        hidden_example()\n\n    >>> hidden_doctest()\n    \"\"\"\n";
        let prose = prose_view(python, SourceType::Code(Language::Python));
        assert!(prose.contains("Stream a response."));
        assert!(!prose.contains("hidden"));

        let rust = "/// Escapes a glob.\n///\n/// ```\n/// assert_eq!(escape(\"hidden\"), \"x\");\n/// ```\nfn escape() {}\n";
        let prose = prose_view(rust, SourceType::Code(Language::Rust));
        assert!(prose.contains("Escapes a glob."));
        assert!(!prose.contains("hidden"));
        assert_eq!(prose.len(), rust.len());
    }

    #[test]
    fn sentences_stop_at_paragraph_boundaries() {
        let source =
            "Installation\n============\n\nRun the installer now. Then restart\n///\nthe shell.";
        let paragraphs: Vec<_> = paragraph_spans(source)
            .iter()
            .map(|span| span.text(source))
            .collect();
        assert_eq!(
            paragraphs,
            [
                "Installation",
                "Run the installer now. Then restart",
                "the shell."
            ]
        );
        let sentences: Vec<_> = sentence_spans(source)
            .iter()
            .map(|span| span.text(source))
            .collect();
        assert_eq!(
            sentences,
            [
                "Installation",
                "Run the installer now.",
                "Then restart",
                "the shell."
            ]
        );
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
