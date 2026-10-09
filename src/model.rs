use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "info" => Ok(Self::Info),
            "warning" | "warn" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            _ => Err(format!("expected info, warning, or error; got {value:?}")),
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => formatter.write_str("info"),
            Self::Warning => formatter.write_str("warning"),
            Self::Error => formatter.write_str("error"),
        }
    }
}

impl Severity {
    #[must_use]
    pub const fn sarif_level(self) -> &'static str {
        match self {
            Self::Info => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Deadweight,
    Papertrail,
    Polygraph,
    Vibecheck,
}

impl std::fmt::Display for Module {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deadweight => formatter.write_str("deadweight"),
            Self::Papertrail => formatter.write_str("papertrail"),
            Self::Polygraph => formatter.write_str("polygraph"),
            Self::Vibecheck => formatter.write_str("vibecheck"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RuleMetadata {
    pub id: &'static str,
    pub module: Module,
    pub description: &'static str,
    pub default_severity: Severity,
    pub default_confidence: Confidence,
    pub message: &'static str,
    pub suggestion: &'static str,
    pub rationale: &'static str,
    pub examples: &'static [&'static str],
    pub false_positives: &'static str,
    /// Plain alternatives for the expressions the rule counts, as `(expression, replacement)`
    /// pairs. A finding names the replacements for the expressions it matched; rules that observe
    /// structure rather than wording leave this empty.
    #[serde(serialize_with = "serialize_replacements")]
    pub replacements: &'static [(&'static str, &'static str)],
}

impl RuleMetadata {
    /// Returns the replacement for an expression the rule matched. Both sides are lowercased and
    /// stripped of surrounding punctuation, and a typographic apostrophe compares equal to `'`.
    #[must_use]
    pub fn replacement_for(&self, matched: &str) -> Option<&'static str> {
        let matched = normalize_expression(matched);
        self.replacements
            .iter()
            .find(|(expression, _)| normalize_expression(expression) == matched)
            .map(|(_, replacement)| *replacement)
    }
}

fn normalize_expression(expression: &str) -> String {
    expression
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_lowercase()
        .replace('\u{2019}', "'")
}

fn serialize_replacements<S: serde::Serializer>(
    replacements: &&'static [(&'static str, &'static str)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;

    #[derive(Serialize)]
    struct Entry {
        expression: &'static str,
        replacement: &'static str,
    }

    let mut sequence = serializer.serialize_seq(Some(replacements.len()))?;
    for (expression, replacement) in *replacements {
        sequence.serialize_element(&Entry {
            expression,
            replacement,
        })?;
    }
    sequence.end()
}

/// A one-based source range. Columns count Unicode scalar values, and `end_column` is the column
/// just past the last character, as in SARIF.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Location {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Location {
    /// An empty range at one position.
    #[must_use]
    pub const fn point(line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            end_line: line,
            end_column: column,
        }
    }

    /// Locates a byte offset and extends the range to the end of that line's content. Rules
    /// report a position, and most of them judge a whole line or a construct that starts on it,
    /// so the line is the narrowest span that holds the evidence.
    #[must_use]
    pub fn at(source: &str, offset: usize) -> Self {
        let mut offset = offset.min(source.len());
        while !source.is_char_boundary(offset) {
            offset -= 1;
        }
        let prefix = &source[..offset];
        let line_start = prefix.rfind('\n').map_or(0, |position| position + 1);
        let line_end = source[offset..]
            .find('\n')
            .map_or(source.len(), |position| offset + position);
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix[line_start..].chars().count() + 1;
        let content_end = source[line_start..line_end].trim_end().chars().count() + 1;
        Self {
            line,
            column,
            end_line: line,
            end_column: content_end.max(column),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Finding {
    pub path: PathBuf,
    pub location: Location,
    pub rule_id: &'static str,
    pub module: Module,
    pub severity: Severity,
    pub confidence: Confidence,
    pub message: String,
    pub evidence: Option<String>,
    pub observation: Option<String>,
    pub suggestion: &'static str,
}

#[cfg(test)]
mod tests {
    use super::Location;

    #[test]
    fn locations_span_the_rest_of_the_line_content() {
        let source = "first\r\n  é = todo()   \r\nlast";
        let offset = source.find('é').expect("marker");
        let location = Location::at(source, offset);
        assert_eq!((location.line, location.column), (2, 3));
        assert_eq!((location.end_line, location.end_column), (2, 13));
        assert_eq!(Location::at(source, source.len()), Location::point(3, 5));
        let blank = Location::at("a\n\nb", 2);
        assert_eq!(blank, Location::point(2, 1));
    }
}
