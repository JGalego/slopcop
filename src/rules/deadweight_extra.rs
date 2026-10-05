use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::{BuiltinRule, Rule, ScanContext, emit, matched_source_has_explanatory_comment};
use crate::analysis::{preceding_text, python_block_ends_at, word_count, words};
use crate::language::{Language, SourceType};
use crate::model::{Confidence, Module, RuleMetadata, Severity};

static DEAD002: RuleMetadata = RuleMetadata {
    id: "DEAD002",
    module: Module::Deadweight,
    description: "Escaped placeholder marker",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "A placeholder marker remains in production-facing code.",
    suggestion: "Implement the missing behavior or link the marker to tracked work with a concrete reason.",
    rationale: "Unowned TODO-style scaffolding is easily mistaken for completed agent output.",
    examples: &["# TODO: implement retry handling"],
    false_positives: "Tracked debt can be intentional; configure this rule for repositories that enforce issue-linked markers separately.",
};

static DEAD003: RuleMetadata = RuleMetadata {
    id: "DEAD003",
    module: Module::Deadweight,
    description: "Explicit unimplemented body",
    default_severity: Severity::Error,
    default_confidence: Confidence::High,
    message: "Executable code contains an explicit unimplemented placeholder.",
    suggestion: "Implement the behavior before shipping, or move the contract to a recognized abstract interface.",
    rationale: "Agent-generated patches often leave language-level placeholder calls behind while surrounding code appears complete.",
    examples: &[
        "todo!()",
        "raise NotImplementedError",
        "throw new Error(\"not implemented\")",
    ],
    false_positives: "Abstract methods and deliberate extension points are excluded when a nearby abstract declaration is visible.",
};

static DEAD004: RuleMetadata = RuleMetadata {
    id: "DEAD004",
    module: Module::Deadweight,
    description: "Meaningless exception fallback",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "An exception is converted to an empty or constant fallback.",
    suggestion: "Handle the expected failure explicitly or return a typed result that preserves the cause.",
    rationale: "Returning None, null, false, or an empty collection for every failure erases useful failure information.",
    examples: &[
        "except Exception:\n    return None",
        "catch (error) { return null; }",
    ],
    false_positives: "Compatibility probes with an explanatory body comment are excluded.",
};

static DEAD005: RuleMetadata = RuleMetadata {
    id: "DEAD005",
    module: Module::Deadweight,
    description: "Empty concrete function",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "A concrete function has an empty body.",
    suggestion: "Implement the function, remove it, or declare the containing interface abstract.",
    rationale: "Empty concrete functions create an API surface that promises behavior but performs none.",
    examples: &["def publish(event):\n    pass", "function publish() {}"],
    false_positives: "Framework hooks with an explanatory body comment are excluded.",
};

static DEAD006: RuleMetadata = RuleMetadata {
    id: "DEAD006",
    module: Module::Deadweight,
    description: "Comment restates adjacent code",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "A comment appears to restate the operation immediately below it.",
    suggestion: "Remove the comment or explain the non-obvious constraint behind the operation.",
    rationale: "Narrating simple syntax increases maintenance cost without preserving intent.",
    examples: &["# Increment the counter\ncounter += 1"],
    false_positives: "Teaching material may intentionally narrate syntax; generated tutorials can demote or disable this rule.",
};

static DEAD007: RuleMetadata = RuleMetadata {
    id: "DEAD007",
    module: Module::Deadweight,
    description: "Duplicated adjacent comment",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "The same comment is repeated in adjacent code.",
    suggestion: "Keep one explanation at the narrowest scope where it remains accurate.",
    rationale: "Repeated comments are common residue from generated edits and drift independently from the code.",
    examples: &["// Validate the request\n// Validate the request"],
    false_positives: "Inline shape annotations, type directives, and visual separators are not standalone comment candidates.",
};

static DEAD008: RuleMetadata = RuleMetadata {
    id: "DEAD008",
    module: Module::Deadweight,
    description: "Name-preserving delegation wrapper",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "A function only forwards its arguments to another function with the same name.",
    suggestion: "Call the underlying function directly, or make the wrapper's policy or compatibility purpose explicit.",
    rationale: "One-line forwarding layers add navigation cost when they introduce no policy, validation, conversion, or stable boundary.",
    examples: &["def save(item):\n    return client.save(item)"],
    false_positives: "Public facades and compatibility shims can be useful even when their current implementation delegates directly.",
};

static DEAD009: RuleMetadata = RuleMetadata {
    id: "DEAD009",
    module: Module::Deadweight,
    description: "Pointless Boolean branch",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "A conditional returns true in one branch and false in the other.",
    suggestion: "Return the condition directly, negating it when necessary.",
    rationale: "Generated code often expands a Boolean expression into ceremonial control flow that obscures the actual condition.",
    examples: &["if ready:\n    return True\nelse:\n    return False"],
    false_positives: "Explicit branches may aid breakpoint placement, but that need should be rare and documented.",
};

static DEAD010: RuleMetadata = RuleMetadata {
    id: "DEAD010",
    module: Module::Deadweight,
    description: "Trivial test assertion",
    default_severity: Severity::Error,
    default_confidence: Confidence::High,
    message: "A test asserts a constant truth rather than observable behavior.",
    suggestion: "Assert a result or side effect produced by the unit under test.",
    rationale: "Always-passing assertions create the appearance of coverage without constraining behavior.",
    examples: &["assert True", "expect(true).toBe(true)", "assert!(true)"],
    false_positives: "Temporary harness smoke checks should not be committed as behavioral tests.",
};

static DEAD011: RuleMetadata = RuleMetadata {
    id: "DEAD011",
    module: Module::Deadweight,
    description: "Duplicated code block",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "A non-trivial block of code is duplicated in the same file.",
    suggestion: "Share the behavior when it represents one policy, or make intentional duplication visibly distinct.",
    rationale: "Generated patches frequently repeat a solved block instead of finding the existing ownership boundary.",
    examples: &["Ten or more equivalent non-empty lines repeated later in one file."],
    false_positives: "Tables, generated sources, and intentionally unrolled hot paths can contain legitimate repeated blocks.",
};

static DEAD012: RuleMetadata = RuleMetadata {
    id: "DEAD012",
    module: Module::Deadweight,
    description: "Empty or self-restating documentation section",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "A documentation section is empty or only restates its heading.",
    suggestion: "Add concrete information, merge the section into a useful neighbor, or remove it.",
    rationale: "A polished heading with no additional information makes documentation look complete while leaving the reader unaided.",
    examples: &["## Configuration\nThis section describes configuration."],
    false_positives: "Outline documents may intentionally contain empty headings while actively being drafted.",
};

static DEAD013: RuleMetadata = RuleMetadata {
    id: "DEAD013",
    module: Module::Deadweight,
    description: "Brittle inventory count",
    default_severity: Severity::Info,
    default_confidence: Confidence::Medium,
    message: "Documentation repeats an exact count of a mutable repository inventory.",
    suggestion: "Name the capabilities or link to the maintained inventory instead of repeating its current size.",
    rationale: "Counts of rules, commands, integrations, and similar catalogs become stale whenever the inventory changes while rarely helping a reader make a decision.",
    examples: &["The registry contains 32 stable rules."],
    false_positives: "Release snapshots, compatibility limits, generated summaries, and fixed protocol cardinalities may require exact counts.",
};

pub(super) fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        boxed(&DEAD002, check_placeholder_marker),
        boxed(&DEAD003, check_unimplemented),
        boxed(&DEAD004, check_exception_fallback),
        boxed(&DEAD005, check_empty_function),
        boxed(&DEAD006, check_redundant_comment),
        boxed(&DEAD007, check_duplicate_comment),
        boxed(&DEAD008, check_delegating_wrapper),
        boxed(&DEAD009, check_boolean_branch),
        boxed(&DEAD010, check_trivial_test),
        boxed(&DEAD011, check_duplicate_block),
        boxed(&DEAD012, check_empty_doc_section),
        boxed(&DEAD013, check_inventory_count),
    ]
}

fn boxed(metadata: &'static RuleMetadata, check_fn: super::CheckFn) -> Box<dyn Rule> {
    Box::new(BuiltinRule { metadata, check_fn })
}

fn check_placeholder_marker(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !matches!(context.source_type, SourceType::Code(_)) {
        return;
    }
    let lower = context.prose().to_ascii_lowercase();
    let mut offsets: Vec<_> = ["todo", "fixme", "hack", "xxx"]
        .iter()
        .flat_map(|marker| find_words(&lower, marker))
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    for offset in offsets {
        emit(context, metadata, findings, offset, None::<String>);
    }
}

fn check_unimplemented(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !matches!(context.source_type, SourceType::Code(_)) {
        return;
    }
    let lower = context.code().to_ascii_lowercase();
    let candidates = ["todo!", "unimplemented!"];
    let mut offsets: Vec<_> = candidates
        .iter()
        .flat_map(|value| lower.match_indices(value).map(|(offset, _)| offset))
        .collect();
    offsets.extend(
        unimplemented_exception_matcher()
            .find_iter(context.code())
            .map(|found| found.start()),
    );

    for marker in ["throw new error", "panic!(", "panic("] {
        for (found, _) in lower.match_indices(marker) {
            let end = lower[found..]
                .find([';', '\n'])
                .map_or(lower.len(), |relative| found + relative);
            let statement = context.source[found..end].to_ascii_lowercase();
            if statement.contains("not implemented") || statement.contains("todo") {
                offsets.push(found);
            }
        }
    }

    offsets.sort_unstable();
    offsets.dedup();
    for offset in offsets {
        let nearby = preceding_text(&lower, offset, 240);
        if nearby.contains("abstract") || nearby.contains("trait ") || nearby.contains("protocol ")
        {
            continue;
        }
        emit(context, metadata, findings, offset, None::<String>);
    }
}

fn unimplemented_exception_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?i)\b(?:raise|throw[ \t]+new)[ \t]+NotImplemented(?:Error|Exception)\b")
            .expect("DEAD003 exception regex must compile")
    })
}

fn check_exception_fallback(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let matcher = match context.source_type {
        SourceType::Code(Language::Python) => python_fallback_matcher(),
        SourceType::Code(Language::JavaScript | Language::TypeScript) => brace_fallback_matcher(),
        _ => return,
    };
    for found in matcher.find_iter(context.code()) {
        if matched_source_has_explanatory_comment(context, found.start(), found.end()) {
            continue;
        }
        emit(context, metadata, findings, found.start(), None::<String>);
    }
}

fn python_fallback_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*except(?:[^\r\n:]*)?:[ \t]*\r?\n[ \t]+return[ \t]+(?:None|False|\[\]|\{\})[ \t]*$")
            .expect("DEAD004 Python regex must compile")
    })
}

fn brace_fallback_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)catch\s*(?:\([^)]*\))?\s*\{\s*return\s+(?:null|false|\[\]|\{\})\s*;?\s*\}")
            .expect("DEAD004 brace regex must compile")
    })
}

fn check_empty_function(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let matcher = match context.source_type {
        SourceType::Code(Language::Python) => python_empty_function_matcher(),
        SourceType::Code(
            Language::JavaScript | Language::TypeScript | Language::Go | Language::Rust,
        ) => brace_empty_function_matcher(),
        _ => return,
    };
    for found in matcher.find_iter(context.code()) {
        if matched_source_has_explanatory_comment(context, found.start(), found.end()) {
            continue;
        }
        if context.source_type == SourceType::Code(Language::Python)
            && !python_block_ends_at(context.code(), found.start(), found.end())
        {
            continue;
        }
        let nearby = preceding_text(context.lower_source(), found.start(), 160);
        if nearby.contains("abstract") || nearby.contains("trait ") || nearby.contains("interface ")
        {
            continue;
        }
        emit(context, metadata, findings, found.start(), None::<String>);
    }
}

fn python_empty_function_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*(?:async[ \t]+)?def[ \t]+[A-Za-z_]\w*\([^\r\n]*\):[ \t]*\r?\n[ \t]+(?:pass|\.\.\.)[ \t]*$")
            .expect("DEAD005 Python regex must compile")
    })
}

fn brace_empty_function_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*(?:(?:pub|export|async|unsafe|extern|static)[ \t]+)*(?:fn|func|function)[ \t]+[A-Za-z_]\w*[^\n{]*\{[ \t]*\}")
            .expect("DEAD005 brace regex must compile")
    })
}

fn check_redundant_comment(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let SourceType::Code(language) = context.source_type else {
        return;
    };
    let lines: Vec<_> = context.source.split_inclusive('\n').collect();
    let mut offset = 0;
    for pair in lines.windows(2) {
        if !is_standalone_line_comment(pair[0], language) {
            offset += pair[0].len();
            continue;
        }
        let comment = context.prose()[offset..offset + pair[0].len()].trim();
        let code = pair[1].trim();
        if let Some(content) = line_comment_content(comment, language) {
            if !content.starts_with(['/', '!', '#']) && comment_restates(content, code) {
                emit(context, metadata, findings, offset, None::<String>);
            }
        }
        offset += pair[0].len();
    }
}

fn comment_restates(comment: &str, code: &str) -> bool {
    if code.is_empty() || comment.len() < 8 {
        return false;
    }
    let ignored = [
        "the", "a", "an", "to", "this", "that", "variable", "value", "we",
    ];
    let comment_words: HashSet<_> = words(comment)
        .into_iter()
        .filter(|word| !ignored.contains(&word.as_str()))
        .collect();
    if comment_words.len() < 2 || comment_words.len() > 8 {
        return false;
    }
    let mut code_words: HashSet<_> = words(code).into_iter().collect();
    if code.contains("+=") {
        code_words.insert("increment".to_owned());
    }
    if code.contains("-=") {
        code_words.insert("decrement".to_owned());
    }
    if code.contains('=') {
        code_words.extend([
            "set".to_owned(),
            "assign".to_owned(),
            "initialize".to_owned(),
        ]);
    }
    if code.trim_start().starts_with("return") {
        code_words.insert("return".to_owned());
    }
    let overlap = comment_words.intersection(&code_words).count();
    overlap >= 2 && overlap * 5 >= comment_words.len() * 4
}

fn check_duplicate_comment(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let SourceType::Code(language) = context.source_type else {
        return;
    };
    let mut previous: Option<(String, usize, usize)> = None;
    let mut offset = 0;
    for (line_number, line) in context.source.split_inclusive('\n').enumerate() {
        if !is_standalone_line_comment(line, language) {
            if !line.trim().is_empty() {
                previous = None;
            }
            offset += line.len();
            continue;
        }
        if let Some(content) = line_comment_content(
            context.prose()[offset..offset + line.len()].trim(),
            language,
        ) {
            let normalized = words(content).join(" ");
            if normalized.len() >= 12 {
                if let Some((prior, _, prior_line)) = &previous {
                    if *prior == normalized && line_number <= prior_line + 2 {
                        emit(context, metadata, findings, offset, None::<String>);
                    }
                }
                previous = Some((normalized, offset, line_number));
            }
        } else if !line.trim().is_empty() {
            previous = None;
        }
        offset += line.len();
    }
}

fn is_standalone_line_comment(line: &str, language: Language) -> bool {
    let trimmed = line.trim_start();
    match language {
        Language::Python | Language::Ruby | Language::Shell => trimmed.starts_with('#'),
        _ => trimmed.starts_with("//"),
    }
}

fn line_comment_content(line: &str, language: Language) -> Option<&str> {
    match language {
        Language::Python | Language::Ruby | Language::Shell => {
            line.strip_prefix('#').map(str::trim)
        }
        _ => line.strip_prefix("//").map(str::trim),
    }
}

fn check_delegating_wrapper(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let matcher = match context.source_type {
        SourceType::Code(Language::Python) => python_delegate_matcher(),
        SourceType::Code(Language::JavaScript | Language::TypeScript) => brace_delegate_matcher(),
        _ => return,
    };
    for captures in matcher.captures_iter(context.code()) {
        let name = captures.name("name").map(|value| value.as_str());
        let call = captures.name("call").map(|value| value.as_str());
        let params = captures
            .name("params")
            .map(|value| normalized_arguments(value.as_str()));
        let args = captures
            .name("args")
            .map(|value| normalized_arguments(value.as_str()));
        if name == call && params == args {
            let offset = captures.get(0).map_or(0, |value| value.start());
            if context.source_type == SourceType::Code(Language::Python)
                && !python_block_ends_at(
                    context.code(),
                    offset,
                    captures.get(0).expect("whole match").end(),
                )
            {
                continue;
            }
            emit(context, metadata, findings, offset, None::<String>);
        }
    }
}

fn python_delegate_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*(?:async[ \t]+)?def[ \t]+(?P<name>[A-Za-z_]\w*)\((?P<params>[^\r\n)]*)\):[ \t]*\r?\n[ \t]+return[ \t]+(?:await[ \t]+)?(?:[A-Za-z_]\w*\.)*(?P<call>[A-Za-z_]\w*)\((?P<args>[^\r\n)]*)\)[ \t]*$")
            .expect("DEAD008 Python regex must compile")
    })
}

fn brace_delegate_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)function\s+(?P<name>[A-Za-z_$][\w$]*)\s*\((?P<params>[^)]*)\)\s*\{\s*return\s+(?:[A-Za-z_$][\w$]*\.)*(?P<call>[A-Za-z_$][\w$]*)\((?P<args>[^)]*)\)\s*;?\s*\}")
            .expect("DEAD008 brace regex must compile")
    })
}

fn normalized_arguments(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "self" && *part != "this")
        .map(|part| {
            part.split([':', '='])
                .next()
                .unwrap_or(part)
                .trim()
                .to_owned()
        })
        .collect()
}

fn check_boolean_branch(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let matcher = match context.source_type {
        SourceType::Code(Language::Python) => python_boolean_matcher(),
        SourceType::Code(
            Language::JavaScript | Language::TypeScript | Language::Java | Language::CSharp,
        ) => brace_boolean_matcher(),
        _ => return,
    };
    for captures in matcher.captures_iter(context.code()) {
        if captures.name("first").map(|value| value.as_str())
            != captures.name("second").map(|value| value.as_str())
        {
            let found = captures.get(0).expect("whole match");
            emit(context, metadata, findings, found.start(), None::<String>);
        }
    }
}

fn python_boolean_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*if[^\r\n]+:[ \t]*\r?\n[ \t]+return[ \t]+(?P<first>True|False)[ \t]*\r?\n[ \t]*else:[ \t]*\r?\n[ \t]+return[ \t]+(?P<second>False|True)[ \t]*$")
            .expect("DEAD009 Python regex must compile")
    })
}

fn brace_boolean_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)if\s*\([^)]*\)\s*\{\s*return\s+(?P<first>true|false)\s*;\s*\}\s*else\s*\{\s*return\s+(?P<second>false|true)\s*;\s*\}")
            .expect("DEAD009 brace regex must compile")
    })
}

fn check_trivial_test(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if !matches!(context.source_type, SourceType::Code(_)) || !is_test_path(context.path) {
        return;
    }
    for found in trivial_test_matcher().find_iter(context.code()) {
        emit(context, metadata, findings, found.start(), None::<String>);
    }
}

fn trivial_test_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(concat!(
            r#"(?im)(?:"#,
            r#"^[ \t]*assert[ \t]+(?:True|1[ \t]*==[ \t]*1)[ \t]*$"#,
            r#"|expect\s*\(\s*(?:"#,
            r#"true\s*\)\s*\.\s*to(?:Be|Equal)\s*\(\s*true"#,
            r#"|false\s*\)\s*\.\s*to(?:Be|Equal)\s*\(\s*false"#,
            r#"|0\s*\)\s*\.\s*to(?:Be|Equal)\s*\(\s*0"#,
            r#"|1\s*\)\s*\.\s*to(?:Be|Equal)\s*\(\s*1"#,
            r#")\s*\)"#,
            r#"|assert!\s*\(\s*true\s*\)"#,
            r#"|assert\.ok\s*\(\s*true\s*\)"#,
            r#"|assert\.(?:equal|strictEqual)\s*\(\s*(?:true\s*,\s*true|false\s*,\s*false|0\s*,\s*0|1\s*,\s*1)\s*\)"#,
            r#"|assert_eq!\s*\(\s*(?:true\s*,\s*true|false\s*,\s*false|0\s*,\s*0|1\s*,\s*1)\s*\)"#,
            r#")"#,
        ))
        .expect("DEAD010 test regex must compile")
    })
}

fn is_test_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    normalized.contains("/test")
        || normalized.starts_with("test")
        || normalized.contains("_test.")
        || normalized.contains(".test.")
        || normalized.contains(".spec.")
}

fn check_duplicate_block(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const BLOCK_LINES: usize = 10;
    if !matches!(context.source_type, SourceType::Code(_)) {
        return;
    }
    let lines: Vec<_> = context.source.lines().collect();
    if lines.len() < BLOCK_LINES * 2 + 2 {
        return;
    }
    let mut offsets = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in context.source.split_inclusive('\n') {
        offsets.push(offset);
        offset += line.len();
    }
    let normalized: Vec<_> = lines
        .iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut reported_keys = HashSet::new();
    let mut reported_end = 0;
    let mut reported_pair_end = None;
    for start in 0..=normalized.len().saturating_sub(BLOCK_LINES) {
        if start < reported_end {
            continue;
        }
        let window = &normalized[start..start + BLOCK_LINES];
        if window.iter().any(String::is_empty)
            || window.iter().map(String::len).sum::<usize>() < 180
        {
            continue;
        }
        let key = window.join("\n");
        if let Some(previous) = seen.get(&key) {
            if start >= previous + BLOCK_LINES {
                let first_report = reported_keys.insert(key.clone());
                let continuation = reported_pair_end.is_some_and(|(previous_end, repeated_end)| {
                    *previous == previous_end && start == repeated_end
                });
                if first_report && !continuation {
                    emit(
                        context,
                        metadata,
                        findings,
                        offsets.get(start).copied().unwrap_or(0),
                        Some(format!(
                            "ten-line block first appears at line {}",
                            previous + 1
                        )),
                    );
                }
                reported_end = start + BLOCK_LINES;
                reported_pair_end = Some((previous + BLOCK_LINES, start + BLOCK_LINES));
            }
        } else {
            seen.insert(key, start);
        }
    }
}

fn check_empty_doc_section(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if context.source_type != SourceType::Documentation {
        return;
    }
    let lines: Vec<_> = context.prose().split_inclusive('\n').collect();
    let mut offset = 0;
    let mut headings = Vec::new();
    for line in &lines {
        if let Some((level, title)) = markdown_heading(line) {
            headings.push((offset, level, title.to_owned()));
        }
        offset += line.len();
    }
    for (index, (heading_offset, level, title)) in headings.iter().enumerate() {
        let content_start = context.source[*heading_offset..]
            .find('\n')
            .map_or(context.source.len(), |relative| {
                heading_offset + relative + 1
            });
        let content_end = headings[index + 1..]
            .iter()
            .find(|(_, next_level, _)| next_level <= level)
            .map_or(context.source.len(), |next| next.0);
        let source_body = context.source[content_start..content_end].trim();
        let prose_body = context.prose()[content_start..content_end].trim();
        if source_body.is_empty() || restates_heading(title, prose_body) {
            emit(context, metadata, findings, *heading_offset, None::<String>);
        }
    }
}

fn check_inventory_count(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    if context.source_type != SourceType::Documentation {
        return;
    }
    for found in inventory_count_matcher().find_iter(context.prose()) {
        emit(context, metadata, findings, found.start(), None::<String>);
    }
}

fn inventory_count_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:contains?|includes?|provides?|offers?|supports?|ships[ \t]+with|has)[ \t]+(?:exactly[ \t]+)?\d{1,4}(?:[ \t]+(?:stable|built-in|default|supported|available|bundled|deterministic|official|first-party)){0,2}[ \t]+(?:rules?|commands?|integrations?|plugins?|templates?|examples?|checks?|detectors?|modules?|providers?|adapters?|workflows?|features?)\b",
        )
        .expect("DEAD013 inventory regex must compile")
    })
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if hashes == 0 || trimmed.as_bytes().get(hashes) != Some(&b' ') {
        return None;
    }
    let title = trimmed[hashes + 1..].trim();
    title
        .chars()
        .any(char::is_alphanumeric)
        .then_some((hashes, title))
}

fn restates_heading(title: &str, body: &str) -> bool {
    if word_count(body) > 12 || body.lines().count() > 2 {
        return false;
    }
    let heading: HashSet<_> = words(title).into_iter().collect();
    let generic = [
        "this",
        "section",
        "describes",
        "covers",
        "explains",
        "provides",
        "information",
        "about",
        "the",
        "following",
        "details",
        "overview",
        "of",
        "an",
        "a",
        "and",
    ];
    let body_words = words(body);
    !body_words.is_empty()
        && body_words
            .iter()
            .all(|word| heading.contains(word) || generic.contains(&word.as_str()))
        && body_words.iter().any(|word| heading.contains(word))
}

fn find_words(haystack: &str, needle: &str) -> Vec<usize> {
    haystack
        .match_indices(needle)
        .filter_map(|(offset, _)| {
            let before = haystack[..offset].chars().next_back();
            let after = haystack[offset + needle.len()..].chars().next();
            let boundary = |character: Option<char>| {
                character.is_none_or(|value| !value.is_alphanumeric() && value != '_')
            };
            (boundary(before) && boundary(after)).then_some(offset)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn findings(rule_id: &str, path: &str, source: &str) -> usize {
        let source_type = crate::language::classify(Path::new(path));
        let context = ScanContext::new(Path::new(path), source, source_type);
        let rule = rules()
            .into_iter()
            .find(|rule| rule.metadata().id == rule_id)
            .expect("registered rule");
        let mut results = Vec::new();
        rule.check(&context, &mut results);
        results.len()
    }

    #[test]
    fn positive_and_negative_examples_are_discriminated() {
        let cases = [
            (
                "DEAD002",
                "app.py",
                "# TODO: implement retries\nrun()\n",
                "# Retries use server hints.\nrun()\n",
            ),
            (
                "DEAD003",
                "app.rs",
                "fn run() { todo!(); }\n",
                "fn run() { execute(); }\n",
            ),
            (
                "DEAD004",
                "app.py",
                "try:\n    run()\nexcept Exception:\n    return None\n",
                "try:\n    run()\nexcept NetworkError as error:\n    raise RetryError() from error\n",
            ),
            (
                "DEAD005",
                "app.py",
                "def publish(event):\n    pass\n",
                "def publish(event):\n    broker.send(event)\n",
            ),
            (
                "DEAD006",
                "app.py",
                "# Increment the counter\ncounter += 1\n",
                "# The server can replay events after reconnect.\ndedupe(events)\n",
            ),
            (
                "DEAD007",
                "app.ts",
                "// Validate the incoming request\n// Validate the incoming request\nvalidate(request);\n",
                "// Validate headers first\n// Decode the body second\nvalidate(request);\n",
            ),
            (
                "DEAD008",
                "app.py",
                "def save(item):\n    return client.save(item)\n",
                "def save(item):\n    validate(item)\n    return client.save(item)\n",
            ),
            (
                "DEAD009",
                "app.py",
                "if ready:\n    return True\nelse:\n    return False\n",
                "return ready\n",
            ),
            (
                "DEAD010",
                "tests/test_app.py",
                "def test_run():\n    assert True\n",
                "def test_run():\n    assert run() == 42\n",
            ),
            (
                "DEAD012",
                "README.md",
                "## Configuration\nThis section describes configuration.\n",
                "## Configuration\nSet `timeout` to the request deadline in milliseconds.\n",
            ),
            (
                "DEAD013",
                "README.md",
                "The registry contains 32 stable rules.\n",
                "The rule index documents every stable rule.\n",
            ),
        ];

        for (rule, path, bad, clean) in cases {
            assert_eq!(findings(rule, path, bad), 1, "{rule} should trigger");
            assert_eq!(findings(rule, path, clean), 0, "{rule} should stay quiet");
        }
    }

    #[test]
    fn raw_string_contents_are_not_adjacent_code_comments() {
        let source = "fn run() { let sample = r#\"\n// Increment the counter\ncounter += 1;\n// Validate the incoming request\n// Validate the incoming request\n\"#; execute(sample); }\n";
        assert_eq!(findings("DEAD006", "app.rs", source), 0);
        assert_eq!(findings("DEAD007", "app.rs", source), 0);
    }

    #[test]
    fn repeated_inline_annotations_are_not_duplicate_comments() {
        let source = "keys = project(x)  # (batch, tokens, groups, width)\nvalues = project(y)  # (batch, tokens, groups, width)\n";
        assert_eq!(findings("DEAD007", "model.py", source), 0);
    }

    #[test]
    fn documented_fallbacks_and_empty_functions_are_not_deadweight() {
        assert_eq!(
            findings(
                "DEAD004",
                "app.py",
                "try:\n    probe()\nexcept CompatibilityError:\n    return None  # Optional compatibility probe.\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD004",
                "app.js",
                "try { probe(); } catch (error) { /* Optional compatibility probe. */ return null; }\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD005",
                "app.py",
                "def on_shutdown():\n    pass  # Framework lifecycle hook.\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD005",
                "app.js",
                "function onShutdown() { /* Framework lifecycle hook. */ }\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD004",
                "app.py",
                "try:\n    probe()\nexcept CompatibilityError:\n    return None  # TODO\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD005",
                "app.py",
                "def on_shutdown():\n    pass  # TODO\n"
            ),
            1
        );
    }

    #[test]
    fn local_rules_report_every_occurrence_and_respect_quiet_boundaries() {
        assert_eq!(
            findings(
                "DEAD003",
                "app.rs",
                "fn first() { todo!(); }\nfn second() { todo!(); }\n"
            ),
            2
        );
        assert_eq!(
            findings(
                "DEAD003",
                "app.rs",
                "fn run() -> &'static str { todo!() }\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD003",
                "app.py",
                "raise NotImplementedError(\"missing behavior\")\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD003",
                "app.py",
                "try:\n    run()\nexcept NotImplementedError:\n    recover()\nmock.side_effect = NotImplementedError\nwith pytest.raises(NotImplementedError):\n    run()\n"
            ),
            0
        );
        assert_eq!(
            findings("DEAD005", "app.py", "def run():\n    pass\n    execute()\n"),
            0
        );
        assert_eq!(
            findings(
                "DEAD005",
                "app.py",
                "def first():\n    pass\ndef second():\n    pass\n"
            ),
            2
        );
        assert_eq!(
            findings("DEAD010", "tests/guide.md", "```python\nassert True\n```\n"),
            0
        );
        assert_eq!(
            findings(
                "DEAD009",
                "app.py",
                "if ready:\n    return True\nelse:\n    return True\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD009",
                "app.js",
                "if (ready) { return false; } else { return false; }"
            ),
            0
        );
        let source = "function run(ready) { if (ready) { throw new Error(\"bad input\"); } throw new Error(\"not implemented\"); }\n";
        let context = ScanContext::new(
            Path::new("app.js"),
            source,
            SourceType::Code(Language::JavaScript),
        );
        let mut results = Vec::new();
        check_unimplemented(&context, &DEAD003, &mut results);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].location,
            context.location(source.rfind("throw").expect("unimplemented throw"))
        );
    }

    #[test]
    fn trivial_assertion_location_starts_on_the_assertion() {
        let source = "def test_contract():\n    \"\"\"Document the contract under test.\"\"\"\n    assert True\n";
        let context = ScanContext::new(
            Path::new("tests/test_contract.py"),
            source,
            SourceType::Code(Language::Python),
        );
        let mut results = Vec::new();
        check_trivial_test(&context, &DEAD010, &mut results);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].location.line, 3);
    }

    #[test]
    fn trivial_assertions_cover_common_test_frameworks() {
        for (path, source) in [
            ("app.test.ts", "expect(false).toBe(false);\n"),
            ("app.test.ts", "expect(1).toEqual(1);\n"),
            ("app.test.js", "assert.ok(true);\n"),
            ("app.test.js", "assert.strictEqual(1, 1);\n"),
            ("tests/app.rs", "assert_eq!(false, false);\n"),
        ] {
            assert_eq!(findings("DEAD010", path, source), 1, "{source}");
        }

        for source in [
            "expect(result).toBe(true);\n",
            "expect(false).toBe(true);\n",
            "assert.strictEqual(actual, expected);\n",
            "assert_eq!(actual, expected);\n",
        ] {
            assert_eq!(findings("DEAD010", "app.test.ts", source), 0, "{source}");
        }
    }

    #[test]
    fn unicode_windows_preserve_source_offsets() {
        let prefix = "fn main() { let é = 1; ";
        let offset = prefix.find('é').expect("identifier") + 1 + 240;
        let source = format!("{prefix}{}todo!(); }}\n", " ".repeat(offset - prefix.len()));
        assert_eq!(findings("DEAD003", "app.rs", &source), 1);
        let source = format!("//é{}\nfn empty() {{}}\n", "x".repeat(158));
        assert_eq!(findings("DEAD005", "app.rs", &source), 1);
        let source = "fn main() { let İ = 1; todo!(); }\n";
        let context = ScanContext::new(
            Path::new("app.rs"),
            source,
            crate::language::classify(Path::new("app.rs")),
        );
        let mut results = Vec::new();
        check_unimplemented(&context, &DEAD003, &mut results);
        assert_eq!(
            results[0].location,
            context.location(source.find("todo!").expect("placeholder"))
        );
    }

    #[test]
    fn duplicated_block_requires_ten_substantial_lines() {
        let block = "let alpha = transform(first_value);\nlet beta = transform(second_value);\nlet gamma = combine(alpha, beta);\nvalidate(gamma, expected_schema);\npersist(gamma, transaction_context);\nnotify(gamma, subscription_registry);\naudit(gamma, compliance_context);\nindex(gamma, search_catalog);\nreplicate(gamma, secondary_region);\nrecord_metrics(gamma, telemetry_context);\n";
        let source = format!("fn one() {{\n{block}}}\nfn two() {{\n{block}}}\n");
        assert_eq!(findings("DEAD011", "app.rs", &source), 1);
        assert_eq!(
            findings("DEAD011", "app.rs", "fn one() {\n    execute();\n}\n"),
            0
        );

        let long_block = (0..20).fold(String::new(), |mut block, index| {
            writeln!(
                block,
                "let value_{index} = transform(input_{index}, validation_context_{index});"
            )
            .expect("write test block");
            block
        });
        let source = format!("fn first() {{\n{long_block}}}\nfn second() {{\n{long_block}}}\n");
        assert_eq!(findings("DEAD011", "app.rs", &source), 1);

        let source = format!(
            "fn first() {{\n{block}}}\nfn second() {{\n{block}}}\nfn third() {{\n{block}}}\n"
        );
        assert_eq!(findings("DEAD011", "app.rs", &source), 1);
    }

    #[test]
    fn documentation_sections_may_contain_subsections_or_code() {
        let nested = "## Release\n\n### Added\n\n- New scanner\n";
        let code_only = "## Example\n\n```sh\nslopcop .\n```\n";
        let decorative = "# QUICK LINKS\n# --------------\nOfficial website: https://example.com\n";

        assert_eq!(findings("DEAD012", "CHANGELOG.md", nested), 0);
        assert_eq!(findings("DEAD012", "README.md", code_only), 0);
        assert_eq!(findings("DEAD012", "README.md", decorative), 0);
    }
}
