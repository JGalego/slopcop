use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::{BuiltinRule, Rule, ScanContext, emit, matched_source_has_explanatory_comment};
use crate::analysis::{indentation, preceding_text, python_block_ends_at, word_count, words};
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
    false_positives: "Tracked debt can be intentional; configure this rule for repositories that enforce issue-linked markers separately. Test code, quoted references such as `todo` or \"TODO:\", lowercase xxx or hack, and a lowercase or capitalized todo or fixme that does not open its comment, such as a Todo app, are excluded.",
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
    false_positives: "Abstract methods, Python methods whose whole body raises NotImplementedError, and raises guarded by a condition are excluded. A thrown message counts only when it says \"not implemented\", contains an uppercase TODO, or opens with todo, so a todo status or a word such as Mastodon does not.",
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
    false_positives: "Compatibility probes with an explanatory body comment, optional imports, and predicates whose false fallback is the negative answer are excluded.",
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
    false_positives: "Framework hooks with an explanatory body comment, decorated handlers, and test doubles in test code are excluded.",
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
    false_positives: "Teaching material may intentionally narrate syntax; generated tutorials can demote or disable this rule. Lines inside longer comments are not candidates.",
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
    false_positives: "Inline shape annotations, type directives, documentation comments, and visual separators are not standalone comment candidates.",
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
    false_positives: "Public facades and compatibility shims can be useful even when their current implementation delegates directly; methods that expose a member object's operation and decorated handlers are excluded.",
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
    false_positives: "Tables, generated sources, and intentionally unrolled hot paths can contain legitimate repeated blocks. Comments, embedded text, and test code are excluded.",
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
    false_positives: "Outline documents may intentionally contain empty headings while actively being drafted. Stacked headings that share one body are excluded.",
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

static DEAD014: RuleMetadata = RuleMetadata {
    id: "DEAD014",
    module: Module::Deadweight,
    description: "Redundant exception rethrow",
    default_severity: Severity::Warning,
    default_confidence: Confidence::High,
    message: "An exception handler only rethrows the same failure without adding behavior.",
    suggestion: "Remove the handler, or add the translation, cleanup, or context that justifies it.",
    rationale: "Catch-and-rethrow scaffolding adds indentation and suggests handling while preserving the original control flow unchanged.",
    examples: &[
        "catch (error) { throw error; }",
        "except NetworkError:\n    raise",
    ],
    false_positives: "A temporary debugger breakpoint may use this shape during local diagnosis but should not remain committed. A Python rethrow followed by another `except` clause of the same `try` is excluded, because it keeps those exceptions out of the broader handler.",
};

static DEAD015: RuleMetadata = RuleMetadata {
    id: "DEAD015",
    module: Module::Deadweight,
    description: "Log-only exception recovery",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "An exception handler only prints the failure and then continues.",
    suggestion: "Handle or propagate the failure, or document why continuing after this specific error is safe.",
    rationale: "Printing an exception without changing control flow can make a failed operation look successful to its caller.",
    examples: &[
        "catch (error) { console.error(error); }",
        "except OSError as error:\n    print(error)",
    ],
    false_positives: "Best-effort batch processing and interactive command loops may intentionally report one failure and continue.",
};

static DEAD016: RuleMetadata = RuleMetadata {
    id: "DEAD016",
    module: Module::Deadweight,
    description: "Empty interaction handler",
    default_severity: Severity::Error,
    default_confidence: Confidence::High,
    message: "A user-facing event handler is present but performs no action.",
    suggestion: "Implement the interaction or remove the control until it has real behavior.",
    rationale: "An empty JSX event callback creates an interface that appears interactive while silently doing nothing.",
    examples: &["<button onClick={() => {}}>Save</button>"],
    false_positives: "Story fixtures and component tests may use no-op handlers; recognized test files are excluded.",
};

static DEAD017: RuleMetadata = RuleMetadata {
    id: "DEAD017",
    module: Module::Deadweight,
    description: "Canned-success action stub",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "An action-named function returns a literal success result without doing work.",
    suggestion: "Implement the action, or expose an explicit test fixture instead of a production success stub.",
    rationale: "A hard-coded successful response can make an unfinished operation appear complete to callers and tests.",
    examples: &["async function publish(event) { return { ok: true }; }"],
    false_positives: "Result constructors and test factories may intentionally create success values; recognized test files are excluded and only action-like names trigger.",
};

static DEAD018: RuleMetadata = RuleMetadata {
    id: "DEAD018",
    module: Module::Deadweight,
    description: "Repeated procedural step comments",
    default_severity: Severity::Warning,
    default_confidence: Confidence::Medium,
    message: "Code comments repeatedly narrate numbered implementation steps.",
    suggestion: "Remove procedural narration, or extract real phases into functions whose names preserve the intent.",
    rationale: "Step-by-step comments often restate control flow and become stale as the implementation changes.",
    examples: &["// Step 1: Load data\nload();\n// Step 2: Save data\nsave();"],
    false_positives: "Algorithms with a standardized sequence may need numbered phases; one isolated phase label does not trigger.",
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
        boxed(&DEAD014, check_redundant_rethrow),
        boxed(&DEAD015, check_log_only_handler),
        boxed(&DEAD016, check_empty_event_handler),
        boxed(&DEAD017, check_canned_success),
        boxed(&DEAD018, check_procedural_comments),
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
    if !matches!(context.source_type, SourceType::Code(_)) || is_test_code(context.path) {
        return;
    }
    let production = production_end(context);
    let prose = context.prose();
    let lower = context.lower_prose();
    let mut offsets: Vec<_> = ["todo", "fixme"]
        .iter()
        .flat_map(|marker| {
            find_words(lower, marker)
                .into_iter()
                .map(move |offset| (offset, marker.len()))
        })
        // Uppercase markers count anywhere. Other spellings count only when they open the
        // comment, because elsewhere they are usually nouns or names, as in `todo-app`.
        .filter(|(offset, length)| {
            prose[*offset..offset + length]
                .bytes()
                .all(|byte| byte.is_ascii_uppercase())
                || opens_comment(prose, *offset) && !joins_name(&prose[offset + length..])
        })
        .chain(["HACK", "XXX"].iter().flat_map(|marker| {
            find_words(prose, marker)
                .into_iter()
                .map(move |offset| (offset, marker.len()))
        }))
        .filter(|(offset, length)| *offset < production && !is_quoted(prose, *offset, *length))
        .map(|(offset, _)| offset)
        .collect();
    offsets.sort_unstable();
    let mut reported_line = None;
    for offset in offsets {
        let line = context.location(offset).line;
        if reported_line != Some(line) {
            reported_line = Some(line);
            emit(context, metadata, findings, offset, None::<String>);
        }
    }
}

/// Whether only comment punctuation precedes `offset` on its line of the prose view, which keeps
/// comment markers and blanks code.
fn opens_comment(prose: &str, offset: usize) -> bool {
    let line_start = prose[..offset]
        .rfind('\n')
        .map_or(0, |position| position + 1);
    prose[line_start..offset]
        .chars()
        .all(|character| character.is_whitespace() || "#/*!-;<\"'".contains(character))
}

/// Whether the marker at `offset` is quoted, as in the Sphinx `todo` extension, a `'todo'` option
/// value, or labels such as "TODO:". A quoted marker is a reference, not a marker. A triple-quoted
/// docstring that holds only a marker is still a placeholder.
fn is_quoted(prose: &str, offset: usize, length: usize) -> bool {
    let before = &prose[..offset];
    if before.ends_with('`') {
        return true;
    }
    let after = prose[offset + length..].trim_start_matches(':');
    [
        ('"', '"'),
        ('\'', '\''),
        ('\u{201c}', '\u{201d}'),
        ('\u{2018}', '\u{2019}'),
    ]
    .iter()
    .any(|&(open, close)| {
        before.ends_with(open)
            && !before[..before.len() - open.len_utf8()].ends_with(open)
            && after.starts_with(close)
    })
}

/// Whether text continues a name, as in `todo-app`, `todo.py`, or `todo/list`.
fn joins_name(after: &str) -> bool {
    let mut characters = after.chars();
    matches!(characters.next(), Some('-' | '.' | '/'))
        && characters.next().is_some_and(char::is_alphanumeric)
}

/// Returns where test code begins in a source file. Rust keeps unit tests in a trailing
/// `#[cfg(test)]` module; other languages keep them in separate test files.
fn production_end(context: &ScanContext<'_>) -> usize {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    let matcher = MATCHER.get_or_init(|| {
        Regex::new(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s")
            .expect("test module regex must compile")
    });
    matcher
        .find(context.code())
        .map_or(context.source.len(), |found| found.start())
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
            if names_placeholder(&context.source[found..end]) {
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
        if context.source_type == SourceType::Code(Language::Python)
            && (is_python_method_stub(context, offset) || is_python_guard(context.code(), offset))
        {
            continue;
        }
        emit(context, metadata, findings, offset, None::<String>);
    }
}

/// Reports whether the `raise` at `offset` is the whole body of a method, optionally after a
/// docstring. Python documents this as the way base classes declare methods that subclasses
/// must override, so it is an interface declaration rather than a forgotten placeholder.
fn is_python_method_stub(context: &ScanContext<'_>, offset: usize) -> bool {
    let code = context.code();
    let line_start = code[..offset].rfind('\n').map_or(0, |index| index + 1);
    if !code[line_start..offset].trim().is_empty()
        || context
            .line_at(offset)
            .to_ascii_lowercase()
            .contains("todo")
    {
        return false;
    }
    let Some(header_end) = code[..line_start]
        .trim_end()
        .strip_suffix(':')
        .map(str::len)
    else {
        return false;
    };
    let Some(header_start) = python_header_start(code, header_end) else {
        return false;
    };
    let header = &code[header_start..header_end];
    let Some(parameters) = header.split_once('(').map(|(_, rest)| rest.trim_start()) else {
        return false;
    };
    let receiver = parameters
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .next()
        .unwrap_or("");
    receiver == "self" || receiver == "cls"
}

/// Reports whether the statement at `offset` is the body of an `if`, `elif`, or `else` clause, as in
/// `if files: raise NotImplementedError("streamed bodies cannot include files")`.
fn is_python_guard(code: &str, offset: usize) -> bool {
    code[..offset]
        .lines()
        .rev()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .map(str::trim_start)
        .is_some_and(|line| {
            (line.starts_with("if ") || line.starts_with("elif ") || line.starts_with("else"))
                && line.trim_end().ends_with(':')
        })
}

/// Finds the start of the `def` statement whose header ends at `end`, matching parentheses so that
/// multi-line signatures and annotated parameters are handled.
fn python_header_start(code: &str, end: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, byte) in code.as_bytes()[..end].iter().enumerate().rev() {
        match byte {
            b')' | b']' => depth += 1,
            b'(' | b'[' => depth -= 1,
            b'\n' if depth == 0 => {
                let line = code[index + 1..end].trim_start();
                return (line.starts_with("def ") || line.starts_with("async def "))
                    .then_some(index + 1);
            }
            _ => {}
        }
    }
    let line = code[..end].trim_start();
    (line.starts_with("def ") || line.starts_with("async def ")).then_some(0)
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
        if matched_source_has_explanatory_comment(context, found.start(), found.end())
            || is_boolean_probe(context, found.start(), found.end())
            || found.as_str().contains("ImportError")
        {
            continue;
        }
        emit(context, metadata, findings, found.start(), None::<String>);
    }
}

/// Reports whether a `false` fallback is the negative answer of a predicate, such as
/// `is_valid_cidr`, that also returns `true`. The Boolean is the contract, so the exception is the
/// answer rather than lost failure detail.
fn is_boolean_probe(context: &ScanContext<'_>, start: usize, end: usize) -> bool {
    let code = context.code();
    let returns_false = code[start..end]
        .trim_end_matches(|character: char| {
            character.is_whitespace() || matches!(character, ';' | '}')
        })
        .to_ascii_lowercase()
        .ends_with("return false");
    if !returns_false {
        return false;
    }
    if context.source_type == SourceType::Code(Language::Python) {
        return python_enclosing_function(code, start)
            .is_some_and(|body| find_words(body, "return True").into_iter().next().is_some());
    }
    // Brace languages: `try { ...; return true; } catch { return false; }`, or the `true`
    // result directly after the handler.
    let before = code[..start]
        .trim_end()
        .trim_end_matches('}')
        .trim_end()
        .trim_end_matches(';')
        .trim_end()
        .to_ascii_lowercase();
    let after = code[end..].trim_start().to_ascii_lowercase();
    before.strip_suffix("return true").is_some_and(|rest| {
        rest.ends_with(|character: char| !character.is_alphanumeric() && character != '_')
    }) || after.starts_with("return true")
}

/// Returns the code of the innermost Python function containing `offset`.
fn python_enclosing_function(code: &str, offset: usize) -> Option<&str> {
    let line_start = code[..offset].rfind('\n').map_or(0, |index| index + 1);
    let mut indent = indentation(&code[line_start..]);
    let mut cursor = line_start;
    let start = loop {
        let previous = code[..cursor.checked_sub(1)?]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let line = &code[previous..cursor];
        cursor = previous;
        if line.trim().is_empty() || indentation(line) >= indent {
            continue;
        }
        indent = indentation(line);
        let statement = line.trim_start();
        if statement.starts_with("def ") || statement.starts_with("async def ") {
            break previous;
        }
        if indent == 0 || statement.starts_with("class ") {
            return None;
        }
    };
    let mut end = code.len();
    let mut position = code[start..]
        .find('\n')
        .map_or(code.len(), |index| start + index + 1);
    while position < code.len() {
        let next = code[position..]
            .find('\n')
            .map_or(code.len(), |index| position + index + 1);
        let line = &code[position..next];
        if !line.trim().is_empty() && indentation(line) <= indent {
            end = position;
            break;
        }
        position = next;
    }
    Some(&code[start..end])
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
        if is_decorated(context.code(), found.start()) || is_test_code(context.path) {
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
        Regex::new(r"(?m)^[ \t]*(?:(?:pub|export|async|unsafe|extern|static)[ \t]+)*(?:fn|func|function)[ \t]+[A-Za-z_]\w*[^\n]*\{[ \t]*\}[ \t]*;?[ \t]*$")
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
    let is_comment = |line: &str| is_standalone_line_comment(line, language);
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        let start = offset;
        offset += line.len();
        // Narration is a single comment line above the statement it describes. A line inside a
        // longer comment block is part of a larger explanation, list, or commented-out example.
        let Some(next) = lines.get(index + 1) else {
            break;
        };
        if !is_comment(line) || is_comment(next) || index > 0 && is_comment(lines[index - 1]) {
            continue;
        }
        let comment = context.prose()[start..offset].trim();
        if let Some(content) = line_comment_content(comment, language) {
            let code = context.code()[offset..offset + next.len()].trim();
            if !content.starts_with(['/', '!', '#']) && comment_restates(content, code) {
                emit(context, metadata, findings, start, None::<String>);
            }
        }
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
            let normalized = normalized_comment(content);
            if !content.starts_with(['/', '!'])
                && word_count(&normalized) >= 2
                && normalized.len() >= 12
            {
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

/// Normalizes case, spacing, and terminal punctuation while keeping symbols, so commented code
/// such as `assert_eq!(f("a?"), 1)` and `assert_eq!(f("a["), 1)` stays distinct.
fn normalized_comment(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '!', '?', ':', ';'])
        .to_lowercase()
}

/// Reports whether the line before `offset` applies a decorator, which registers the function with
/// a framework such as a router or command group.
fn is_decorated(code: &str, offset: usize) -> bool {
    code[..offset]
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.trim_start().starts_with('@'))
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
        let source = |group| {
            captures
                .name(group)
                .map(|value| &context.source[value.range()])
        };
        let params = source("params").map(normalized_arguments);
        let args = source("args").map(normalized_arguments);
        // `this.router.route(path)` exposes a collaborator's operation on the owning object, which
        // is a facade rather than a redundant layer.
        let member = source("receiver")
            .is_some_and(|receiver| receiver.starts_with("self.") || receiver.starts_with("this."));
        if source("name") == source("call") && params == args && !member {
            let offset = captures.get(0).map_or(0, |value| value.start());
            if is_decorated(context.code(), offset) {
                continue;
            }
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
        Regex::new(r"(?mR)^[ \t]*(?:async[ \t]+)?def[ \t]+(?P<name>[A-Za-z_]\w*)\((?P<params>[^\r\n)]*)\):[ \t]*\r?\n[ \t]+return[ \t]+(?:await[ \t]+)?(?P<receiver>(?:[A-Za-z_]\w*\.)*)(?P<call>[A-Za-z_]\w*)\((?P<args>[^\r\n)]*)\)[ \t]*$")
            .expect("DEAD008 Python regex must compile")
    })
}

fn brace_delegate_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)function\s+(?P<name>[A-Za-z_$][\w$]*)\s*\((?P<params>[^)]*)\)\s*\{\s*return\s+(?P<receiver>(?:[A-Za-z_$][\w$]*\.)*)(?P<call>[A-Za-z_$][\w$]*)\((?P<args>[^)]*)\)\s*;?\s*\}")
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

fn is_named_test_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.starts_with("test_")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains("_spec.")
        || name.contains(".spec.")
        || name.contains(".stories.")
}

/// Reports whether a file holds tests: a test-named file, or any file in a test directory.
/// Fixture directories hold sample inputs that stand in for application code, so their files
/// count as tests only when named like one.
fn is_test_code(path: &Path) -> bool {
    let directories: Vec<_> = path
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_ascii_lowercase)
        .collect();
    let in_test_directory = directories.iter().any(|name| {
        matches!(
            name.as_str(),
            "test" | "tests" | "__tests__" | "spec" | "specs"
        )
    }) && !directories
        .iter()
        .any(|name| matches!(name.as_str(), "fixtures" | "testdata"));
    in_test_directory || is_named_test_file(path)
}

fn check_duplicate_block(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    const BLOCK_LINES: usize = 10;
    if !matches!(context.source_type, SourceType::Code(_)) || is_test_code(context.path) {
        return;
    }
    // Tests repeat their setup so that each case reads on its own.
    let lines: Vec<_> = context.source[..production_end(context)].lines().collect();
    if lines.len() < BLOCK_LINES * 2 + 2 {
        return;
    }
    let mut offsets = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in context.source.split_inclusive('\n') {
        offsets.push(offset);
        offset += line.len();
    }
    // A line that is only comment or literal text is blank in the code view and breaks a block,
    // so repeated documentation and embedded text never count as duplicated code.
    let normalized: Vec<_> = lines
        .iter()
        .zip(context.code().lines())
        .map(|(line, code)| {
            if code.trim().is_empty() {
                String::new()
            } else {
                line.split_whitespace().collect::<Vec<_>>().join(" ")
            }
        })
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
        // Consecutive headings of one level, such as overloads of one API, share the next body.
        let stacked = headings
            .get(index + 1)
            .is_some_and(|(next_offset, next_level, _)| {
                *next_offset == content_start && next_level == level
            });
        let source_body = context.source[content_start..content_end].trim();
        let prose_body = context.prose()[content_start..content_end].trim();
        if !stacked && (source_body.is_empty() || restates_heading(title, prose_body)) {
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

fn check_redundant_rethrow(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    match context.source_type {
        SourceType::Code(Language::Python) => {
            for found in python_rethrow_matcher().find_iter(context.code()) {
                if python_block_ends_at(context.code(), found.start(), found.end())
                    && !python_shields_later_handler(context.code(), found.start(), found.end())
                {
                    emit(context, metadata, findings, found.start(), None::<String>);
                }
            }
        }
        SourceType::Code(Language::JavaScript | Language::TypeScript) => {
            for captures in brace_rethrow_matcher().captures_iter(context.code()) {
                if captures.name("caught").map(|value| value.as_str())
                    != captures.name("thrown").map(|value| value.as_str())
                {
                    continue;
                }
                let found = captures.get(0).expect("whole rethrow match");
                emit(context, metadata, findings, found.start(), None::<String>);
            }
        }
        _ => {}
    }
}

/// Whether another `except` clause of the same `try` follows the handler. A bare `raise` there
/// keeps the named exceptions out of the broader handler below, so removing it changes behavior.
fn python_shields_later_handler(code: &str, start: usize, end: usize) -> bool {
    let handler_indent = indentation(&code[start..]);
    code[end..]
        .lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            indentation(line) == handler_indent
                && line
                    .trim_start()
                    .strip_prefix("except")
                    .is_some_and(|rest| {
                        rest.starts_with(|character: char| {
                            character.is_whitespace() || matches!(character, ':' | '*' | '(')
                        })
                    })
        })
}

fn python_rethrow_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*except(?:[^\r\n:]*)?:[ \t]*\r?\n[ \t]+raise[ \t]*$")
            .expect("DEAD014 Python regex must compile")
    })
}

fn brace_rethrow_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)catch\s*\(\s*(?P<caught>[A-Za-z_$][\w$]*)\s*\)\s*\{\s*throw\s+(?P<thrown>[A-Za-z_$][\w$]*)\s*;?\s*\}")
            .expect("DEAD014 brace regex must compile")
    })
}

fn check_log_only_handler(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let (matcher, require_block_end) = match context.source_type {
        SourceType::Code(Language::Python) => (python_log_only_matcher(), true),
        SourceType::Code(Language::JavaScript | Language::TypeScript) => {
            (brace_log_only_matcher(), false)
        }
        _ => return,
    };
    for found in matcher.find_iter(context.code()) {
        let documented =
            matched_source_has_explanatory_comment(context, found.start(), found.end());
        let incomplete_match =
            require_block_end && !python_block_ends_at(context.code(), found.start(), found.end());
        if !documented && !incomplete_match {
            emit(context, metadata, findings, found.start(), None::<String>);
        }
    }
}

fn python_log_only_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*except(?:[^\r\n:]*)?:[ \t]*\r?\n[ \t]+print\([^\r\n]*\)[ \t]*$")
            .expect("DEAD015 Python regex must compile")
    })
}

fn brace_log_only_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)catch\s*(?:\([^)]*\))?\s*\{\s*console\.(?:log|warn|error)\s*\([^;{}]*\)\s*;?\s*\}")
            .expect("DEAD015 brace regex must compile")
    })
}

fn check_empty_event_handler(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_javascript_matcher(context, metadata, findings, empty_event_handler_matcher());
}

fn check_javascript_matcher(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
    matcher: &Regex,
) {
    if !matches!(
        context.source_type,
        SourceType::Code(Language::JavaScript | Language::TypeScript)
    ) || is_named_test_file(context.path)
    {
        return;
    }
    for found in matcher.find_iter(context.code()) {
        emit(context, metadata, findings, found.start(), None::<String>);
    }
}

fn empty_event_handler_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(
            r"(?s)\bon[A-Z][A-Za-z0-9_]*\s*=\s*\{\s*(?:async\s+)?\([^)]*\)\s*=>\s*\{\s*\}\s*\}",
        )
        .expect("DEAD016 event-handler regex must compile")
    })
}

fn check_canned_success(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    check_javascript_matcher(context, metadata, findings, canned_success_matcher());
}

fn canned_success_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(concat!(
            r"(?ms)^[ \t]*",
            r"(?:(?:export|public|private|protected|static)[ \t]+)*",
            r"(?:async[ \t]+)?(?:function[ \t]+)?",
            r"(?:handle[A-Z][A-Za-z0-9_$]*|save|submit|create|update|remove|send|publish|process|execute)",
            r"\s*\([^)]*\)(?:\s*:\s*[^\n{]+)?\s*\{\s*",
            r"return\s*\{\s*(?:ok|success)\s*:\s*true\s*,?\s*\}\s*;?\s*\}",
        ))
        .expect("DEAD017 canned-success regex must compile")
    })
}

fn check_procedural_comments(
    context: &ScanContext<'_>,
    metadata: &'static RuleMetadata,
    findings: &mut Vec<crate::model::Finding>,
) {
    let SourceType::Code(language) = context.source_type else {
        return;
    };
    let mut hits = Vec::new();
    let mut offset = 0;
    for line in context.source.split_inclusive('\n') {
        if is_standalone_line_comment(line, language) {
            let prose_line = context.prose()[offset..offset + line.len()].trim();
            if line_comment_content(prose_line, language)
                .is_some_and(|content| procedural_comment_matcher().is_match(content))
            {
                hits.push(offset);
            }
        }
        offset += line.len();
    }
    if hits.len() >= 2 {
        emit(
            context,
            metadata,
            findings,
            hits[0],
            Some(format!("observed {} numbered step comments", hits.len())),
        );
    }
}

fn procedural_comment_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?i)^[-=*#~ \t]*(?:step|phase)[ \t]+\d+[ \t]*(?:[:.)=–—-]|$)")
            .expect("DEAD018 procedural-comment regex must compile")
    })
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
    // Four or more columns of indentation make the line an indented code block.
    if indentation(line) > 3 {
        return None;
    }
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

/// Whether a thrown message marks missing work: "not implemented", an uppercase `TODO`, or a
/// message that opens with `todo`. Elsewhere a lowercase todo is the domain noun, as in
/// "move the card to todo", and a substring match would also catch words such as "Mastodon".
fn names_placeholder(statement: &str) -> bool {
    if statement.to_ascii_lowercase().contains("not implemented")
        || !find_words(statement, "TODO").is_empty()
    {
        return true;
    }
    statement
        .find(['"', '\'', '`'])
        .map(|quote| statement[quote + 1..].trim_start().to_ascii_lowercase())
        .is_some_and(|message| find_words(&message, "todo").first() == Some(&0))
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
    fn placeholder_markers_exclude_names_quoted_references_and_tests() {
        assert_eq!(
            findings("DEAD002", "app.py", "# XXX handle auth-int.\nrun()\n"),
            1
        );
        assert_eq!(
            findings(
                "DEAD002",
                "app.py",
                "# Exclude --xxx and --yyy after --aaa.\nrun()\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD002",
                "conf.py",
                "# If true, `todo` produces output.\nrun()\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD002",
                "tests/regression.rs",
                "// TODO: re-enable\nfn run() {}\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD002",
                "tasks.py",
                "# todo-null -- in-memory Todo REST API. A body of null must not crash.\nrun()\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD002",
                "server.py",
                "def boot():\n    \"\"\"Boot the produced Todo server on a free port.\"\"\"\n"
            ),
            0
        );
        assert_eq!(
            findings("DEAD002", "app.js", "run(); // todo: handle retries\n"),
            1
        );
        assert_eq!(findings("DEAD002", "app.py", "# Fixme later.\nrun()\n"), 1);
        assert_eq!(
            findings(
                "DEAD002",
                "app.py",
                "# TODO: retry. FIXME: and log.\nrun()\n"
            ),
            1,
            "one line yields one finding"
        );
        for source in [
            "# Reject literal \"lorem ipsum\" / \"TODO\" / \"TBD\" text on a live slide.\nrun()\n",
            "// 'todo' - show a11y violations in the test UI only\nrun();\n",
            "// Preserve user labels such as \"TODO:\" unless they open the line.\nrun();\n",
            "// Flag \u{201c}FIXME\u{201d} notes left in generated slides.\nrun();\n",
        ] {
            let path = if source.starts_with('#') {
                "app.py"
            } else {
                "app.js"
            };
            assert_eq!(findings("DEAD002", path, source), 0, "{source}");
        }
        for source in [
            "def load():\n    \"\"\"TODO\"\"\"\n    return None\n",
            "# \"TODO: explain the daemon\" stays a marker.\nrun()\n",
        ] {
            assert_eq!(findings("DEAD002", "app.py", source), 1, "{source}");
        }
        let module = "struct Sink {\n    #[cfg(test)]\n    line_term: u8,\n}\n// TODO: amortize allocation.\nfn run() {}\n#[cfg(test)]\nmod tests {\n    // TODO: cover CRLF.\n}\n";
        assert_eq!(findings("DEAD002", "src/sink.rs", module), 1);
    }

    #[test]
    fn python_interface_methods_and_guards_are_not_placeholders() {
        let interface = "class Storage:\n    def load(self, key):\n        \"\"\"Return the value.\"\"\"\n        raise NotImplementedError\n\n    def save(\n        self, key: str, value: bytes\n    ) -> None:\n        raise NotImplementedError()\n";
        assert_eq!(findings("DEAD003", "storage.py", interface), 0);
        let guard = "def stream(body, files):\n    if files:\n        raise NotImplementedError(\"no files\")\n    send(body)\n";
        assert_eq!(findings("DEAD003", "stream.py", guard), 0);
        assert_eq!(
            findings(
                "DEAD003",
                "app.py",
                "def load(key):\n    raise NotImplementedError\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD003",
                "app.py",
                "class Store:\n    def load(self):\n        raise NotImplementedError(\"TODO\")\n"
            ),
            1
        );
    }

    #[test]
    fn boolean_predicates_answer_with_their_fallback() {
        for (path, source) in [
            (
                "net.py",
                "def is_ipv4(value):\n    try:\n        parse(value)\n    except ValueError:\n        return False\n    return True\n",
            ),
            (
                "net.py",
                "def has_codec(name):\n    try:\n        lookup(name)\n        return True\n    except LookupError:\n        return False\n",
            ),
            (
                "net.py",
                "def has_winreg():\n    try:\n        import winreg\n    except ImportError:\n        return None\n",
            ),
            (
                "regex.js",
                "function isValid(source) {\n  try {\n    new RegExp(source)\n    return true\n  } catch (e) {\n    return false\n  }\n}\n",
            ),
            (
                "regex.js",
                "function isValid(source) {\n  try { compile(source); } catch (e) { return false; }\n  return true;\n}\n",
            ),
        ] {
            assert_eq!(findings("DEAD004", path, source), 0, "{source}");
        }
        let lossy = "def is_ready():\n    try:\n        probe()\n    except OSError:\n        return None\n    return True\n";
        assert_eq!(findings("DEAD004", "net.py", lossy), 1);
    }

    #[test]
    fn decorated_handlers_test_doubles_and_signature_braces_are_not_empty_functions() {
        assert_eq!(
            findings(
                "DEAD005",
                "app.py",
                "@app.route(\"/\")\ndef index():\n    pass\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD005",
                "tests/apps/factory.py",
                "def no_app():\n    pass\n"
            ),
            0
        );
        for source in [
            "func Gt(a interface{}, b interface{}) bool {\n\treturn true\n}\n",
            "function h(tag, props = {}, ...children) {\n  return tag;\n}\n",
        ] {
            let path = if source.starts_with("func") {
                "cobra.go"
            } else {
                "app.js"
            };
            assert_eq!(findings("DEAD005", path, source), 0, "{source}");
        }
        assert_eq!(
            findings("DEAD005", "cobra.go", "func Noop(a interface{}) {}\n"),
            1
        );
    }

    #[test]
    fn redundant_comments_are_single_lines_above_executable_code() {
        let examples = "// $ curl http://localhost:3000/notfound\n// $ curl http://localhost:3000/notfound -H \"Accept: text/plain\"\napp.listen(3000);\n";
        assert_eq!(findings("DEAD006", "app.js", examples), 0);
        let tail = "// Building the parser is cheap: we get a static parser from\n// `Parser::new()`.\nParser::new().find(name);\n";
        assert_eq!(findings("DEAD006", "flags.rs", tail), 0);
        let data = "CODES = {\n    # Server error.\n    500: (\"internal_server_error\", \"server_error\"),\n}\n";
        assert_eq!(findings("DEAD006", "codes.py", data), 0);
        assert_eq!(
            findings(
                "DEAD006",
                "app.js",
                "run();\n// parse type\nvar parsed = contentType.parse(type);\n"
            ),
            1
        );
    }

    #[test]
    fn duplicate_comments_compare_text_including_symbols() {
        let examples = "/// ```\n/// assert_eq!(escape(\"foo*bar\"), \"foo[*]bar\");\n/// assert_eq!(escape(\"foo?bar\"), \"foo[?]bar\");\n/// ```\nfn escape() {}\n";
        assert_eq!(findings("DEAD007", "lib.rs", examples), 0);
        let commented = "// println!(\"{:#?}\", analysis(r\"foo|bar\"));\n// println!(\"{:#?}\", analysis(r\"foo\"));\nrun();\n";
        assert_eq!(findings("DEAD007", "lib.rs", commented), 0);
        assert_eq!(
            findings(
                "DEAD007",
                "app.ts",
                "// Validate the incoming request.\n// validate the incoming request\nvalidate(request);\n"
            ),
            1
        );
    }

    #[test]
    fn delegation_compares_literal_arguments_and_allows_member_facades() {
        assert_eq!(
            findings(
                "DEAD008",
                "app.py",
                "@app.route(\"/get\")\ndef get():\n    return session.get(\"user\")\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD008",
                "app.py",
                "def get():\n    return session.get(\"user\")\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD008",
                "app.js",
                "app.route = function route(path) { return this.router.route(path); };\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD008",
                "app.py",
                "class App:\n    def route(self, path):\n        return self.router.route(path)\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD008",
                "app.js",
                "function save(item) { return client.save(item); }\n"
            ),
            1
        );
    }

    #[test]
    fn duplicated_blocks_ignore_documentation_and_tests() {
        let docs = (0..10).fold(String::new(), |mut block, index| {
            writeln!(
                block,
                "/// Line {index} explains the shared platform behavior of hidden entries."
            )
            .expect("write doc block");
            block
        });
        let source =
            format!("{docs}fn first() {{}}\n{docs}fn second() {{}}\n{docs}fn third() {{}}\n");
        assert_eq!(findings("DEAD011", "src/pathutil.rs", &source), 0);

        let block = "let alpha = transform(first_value);\nlet beta = transform(second_value);\nlet gamma = combine(alpha, beta);\nvalidate(gamma, expected_schema);\npersist(gamma, transaction_context);\nnotify(gamma, subscription_registry);\naudit(gamma, compliance_context);\nindex(gamma, search_catalog);\nreplicate(gamma, secondary_region);\nrecord_metrics(gamma, telemetry_context);\n";
        let tests = format!(
            "fn run() {{}}\n#[cfg(test)]\nmod tests {{\nfn one() {{\n{block}}}\nfn two() {{\n{block}}}\n}}\n"
        );
        assert_eq!(findings("DEAD011", "src/printer.rs", &tests), 0);
        let source = format!("fn one() {{\n{block}}}\nfn two() {{\n{block}}}\n");
        assert_eq!(findings("DEAD011", "tests/printer.rs", &source), 0);
        assert_eq!(findings("DEAD011", "src/printer.rs", &source), 1);
    }

    #[test]
    fn stacked_and_indented_markdown_lines_are_not_empty_sections() {
        let stacked =
            "### ky.get(input)\n### ky.post(input)\n\nSends a request with the method name.\n";
        assert_eq!(findings("DEAD012", "readme.md", stacked), 0);
        let indented =
            "## Example\n\nRun the server:\n\n    # Start on port 8000.\n    serve(8000)\n";
        assert_eq!(findings("DEAD012", "README.md", indented), 0);
        assert_eq!(
            findings(
                "DEAD012",
                "README.md",
                "## Configuration\n\n## Usage\n\nRun it.\n"
            ),
            1
        );
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
    fn thrown_todo_counts_only_as_a_marker() {
        for source in [
            "function f() { throw new Error(\"Mastodon login failed\"); }\n",
            "function f() { throw new Error(\"specified cards must move to todo.\"); }\n",
            "function f() { if (!todos) throw new Error('expected set_todos tool'); }\n",
            "func f() { panic(\"todos: missing owner\") }\n",
        ] {
            let path = if source.starts_with("func") {
                "app.go"
            } else {
                "app.js"
            };
            assert_eq!(findings("DEAD003", path, source), 0, "{source}");
        }
        for source in [
            "function f() { throw new Error(\"TODO\"); }\n",
            "function f() { throw new Error('todo: wire the exporter'); }\n",
            "function f() { throw new Error(`Export is a TODO for ${format}`); }\n",
        ] {
            assert_eq!(findings("DEAD003", "app.js", source), 1, "{source}");
        }
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

    #[test]
    fn redundant_rethrows_add_no_exception_behavior() {
        assert_eq!(
            findings(
                "DEAD014",
                "app.py",
                "try:\n    run()\nexcept NetworkError:\n    raise\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD014",
                "app.ts",
                "try { run(); } catch (error) { throw error; }\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD014",
                "app.ts",
                "try { run(); } catch (error) { throw new RetryError(error); }\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD014",
                "app.py",
                "try:\n    run()\nexcept NetworkError:\n    cleanup()\n    raise\n"
            ),
            0
        );
        // The rethrow keeps these exceptions out of the broad handler that follows.
        for source in [
            "try:\n    run()\nexcept asyncio.CancelledError:\n    raise\nexcept Exception as error:\n    log(error)\n",
            "try:\n    run()\nexcept ConflictError, NotFoundError:\n\n    raise\n\nexcept Exception:\n    recover()\n",
            "def save():\n    try:\n        run()\n    except KeyError:\n        raise\n    except* ValueError:\n        recover()\n",
        ] {
            assert_eq!(findings("DEAD014", "app.py", source), 0, "{source}");
        }
        assert_eq!(
            findings(
                "DEAD014",
                "app.py",
                "try:\n    run()\nexcept NetworkError:\n    raise\nfinally:\n    close()\nexcept_count = 0\n"
            ),
            1,
            "a finally clause or a later name that starts with except shields nothing"
        );
    }

    #[test]
    fn log_only_handlers_are_distinct_from_real_recovery() {
        assert_eq!(
            findings(
                "DEAD015",
                "app.py",
                "try:\n    run()\nexcept OSError as error:\n    print(error)\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD015",
                "app.js",
                "try { run(); } catch (error) { console.error(error); }\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD015",
                "app.js",
                "try { run(); } catch (error) { console.error(error); recover(error); }\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD015",
                "app.py",
                "try:\n    run()\nexcept OSError as error:\n    logger.exception(\"run failed\")\n"
            ),
            0
        );
    }

    #[test]
    fn empty_jsx_handlers_exclude_tests_and_real_callbacks() {
        assert_eq!(
            findings(
                "DEAD016",
                "src/button.tsx",
                "export const Button = () => <button onClick={() => {}}>Save</button>;\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD016",
                "src/button.tsx",
                "export const Button = () => <button onClick={() => save()}>Save</button>;\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD016",
                "tests/button.test.tsx",
                "const control = <button onClick={() => {}}>Save</button>;\n"
            ),
            0
        );
    }

    #[test]
    fn canned_success_requires_an_action_name_and_empty_behavior() {
        assert_eq!(
            findings(
                "DEAD017",
                "src/publish.ts",
                "export async function publish(event) { return { ok: true }; }\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD017",
                "src/publish.ts",
                "export async function publish(event) { await broker.send(event); return { ok: true }; }\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD017",
                "src/results.ts",
                "function success() { return { ok: true }; }\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD017",
                "tests/publish.test.ts",
                "function publish() { return { ok: true }; }\n"
            ),
            0
        );
    }

    #[test]
    fn procedural_comments_require_repeated_numbered_steps() {
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.ts",
                "// Step 1: Load records\nloadRecords();\n// Step 2: Save records\nsaveRecords();\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.py",
                "# Phase 1 - Parse input\nparse_input()\n# Phase 2 - Write output\nwrite_output()\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.ts",
                "// ===== Step 1: Setup =====\nsetup();\n// --- Step 2) Run ---\nrun();\n// Step 3\nfinish();\n"
            ),
            1
        );
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.rs",
                "/// Step 1: Documented algorithm stage.\n/// Step 2: Documented algorithm stage.\nfn run() {}\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.ts",
                "// Step 1 of the handshake is optional.\nhello();\n// Step 2 depends on the server certificate.\nverify();\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.ts",
                "// Phase 1: Parse untrusted input before validation.\nparseInput();\n"
            ),
            0
        );
        assert_eq!(
            findings(
                "DEAD018",
                "src/pipeline.ts",
                "// Parse before validation because decoding changes byte offsets.\nparseInput();\n// Persist only after every record validates.\nsaveRecords();\n"
            ),
            0
        );
    }
}
