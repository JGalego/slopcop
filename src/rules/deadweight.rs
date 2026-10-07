use std::sync::OnceLock;

use regex::Regex;

use super::{Rule, ScanContext, matched_source_has_explanatory_comment};
use crate::analysis::{indentation, python_block_ends_at};
use crate::language::{Language, SourceType};
use crate::model::{Confidence, Finding, Module, RuleMetadata, Severity};

pub struct EmptyExceptionHandler;

static METADATA: RuleMetadata = RuleMetadata {
    id: "DEAD001",
    module: Module::Deadweight,
    description: "Empty exception handler",
    default_severity: Severity::Error,
    default_confidence: Confidence::High,
    message: "Exception is swallowed without logging, rethrowing, or handling.",
    suggestion: "Handle the failure, rethrow it, or document a narrow and intentional exception.",
    rationale: "Silent exception handling hides failures and turns debugging evidence into an unexplained fallback.",
    examples: &[
        "except NetworkError:\n    pass",
        "catch (error) {}",
        "} catch let error as IOError {}",
    ],
    false_positives: "A deliberately ignored exception with an explanatory body comment or a catch parameter named `ignored`, `ignore`, or `_`, an optional import, an exhausted iterator, or a guarded block that always raises or fails the test is excluded; other undocumented handlers remain findings.",
};

impl Rule for EmptyExceptionHandler {
    fn metadata(&self) -> &'static RuleMetadata {
        &METADATA
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        let matcher = match context.source_type {
            SourceType::Code(Language::Python) => python_matcher(),
            SourceType::Code(
                Language::JavaScript
                | Language::TypeScript
                | Language::Java
                | Language::Kotlin
                | Language::CSharp
                | Language::Cpp
                | Language::Php,
            ) => brace_matcher(),
            SourceType::Code(Language::Swift) => swift_matcher(),
            _ => return,
        };

        for matched in matcher.find_iter(context.code()) {
            if matched_source_has_explanatory_comment(context, matched.start(), matched.end()) {
                continue;
            }
            if context.source_type == SourceType::Code(Language::Python)
                && !python_block_ends_at(context.code(), matched.start(), matched.end())
            {
                continue;
            }
            if is_optional_import(matched.as_str())
                || is_iteration_end(matched.as_str())
                || names_ignored_parameter(matched.as_str())
                || holds_string_literal(context, matched.start(), matched.end())
                || raises_deliberately(context, matched.start())
            {
                continue;
            }
            findings.push(Finding {
                path: context.path.to_path_buf(),
                location: context.location(matched.start()),
                rule_id: METADATA.id,
                module: METADATA.module,
                severity: METADATA.default_severity,
                confidence: METADATA.default_confidence,
                message: METADATA.message.to_owned(),
                evidence: Some(context.line_at(matched.start())),
                observation: None,
                suggestion: METADATA.suggestion,
            });
        }
    }
}

/// Whether the match holds a string literal that the code view blanked. A Kotlin `try` is an
/// expression, so `catch (e: IOException) { "offline" }` yields a value rather than an empty body.
/// String literals are blank in the code and prose views alike; comments survive in the prose view.
fn holds_string_literal(context: &ScanContext<'_>, start: usize, end: usize) -> bool {
    let (source, code, prose) = (
        context.source.as_bytes(),
        context.code().as_bytes(),
        context.prose().as_bytes(),
    );
    (start..end).any(|index| {
        !source[index].is_ascii_whitespace()
            && code[index].is_ascii_whitespace()
            && prose[index].is_ascii_whitespace()
    })
}

/// `except ImportError: pass` is the idiom for an optional dependency: the import is the probe.
/// Whether a catch clause names its parameter `ignored`, `ignore`, or `_`, as in Java's
/// `catch (NameNotFoundException ignored) {}`. The name documents the intent like a body comment.
fn names_ignored_parameter(handler: &str) -> bool {
    handler
        .strip_prefix("catch")
        .and_then(|rest| rest.trim_start().strip_prefix('('))
        .and_then(|rest| rest.split(')').next())
        .is_some_and(|parameter| {
            parameter
                .split(|character: char| !character.is_alphanumeric() && character != '_')
                .any(|word| matches!(word, "ignored" | "ignore" | "_"))
        })
}

fn is_optional_import(handler: &str) -> bool {
    let clause = handler.lines().next().unwrap_or("");
    clause.contains("ImportError") || clause.contains("ModuleNotFoundError")
}

/// `except StopIteration: pass` ends iteration over an exhausted generator, which is control flow
/// rather than a hidden failure.
fn is_iteration_end(handler: &str) -> bool {
    handler
        .lines()
        .next()
        .is_some_and(|clause| clause.contains("StopIteration"))
}

/// Reports whether the guarded block always ends by raising: its last statement raises or fails
/// the test, as in a Swift `do` block that ends with `XCTFail(...)` before it catches the expected
/// error. Tests use this shape to put an exception in flight or to expect one, so discarding it is
/// the intended behavior.
fn raises_deliberately(context: &ScanContext<'_>, handler_start: usize) -> bool {
    let code = context.code();
    let before = code[..handler_start].trim_end();
    if context.source_type != SourceType::Code(Language::Python) {
        let Some(body) = before.strip_suffix('}') else {
            return false;
        };
        // The brace that opens the guarded block, past any closures nested inside it.
        let mut depth = 0_usize;
        let Some(open) = body.bytes().rposition(|byte| match byte {
            b'}' => {
                depth += 1;
                false
            }
            b'{' if depth == 0 => true,
            b'{' => {
                depth -= 1;
                false
            }
            _ => false,
        }) else {
            return false;
        };
        let opener = body[..open].trim_end();
        let opened_by = |keyword: &str| {
            opener.strip_suffix(keyword).is_some_and(|rest| {
                !rest.ends_with(|character: char| character.is_alphanumeric() || character == '_')
            })
        };
        let last = body[open + 1..]
            .trim()
            .trim_end_matches(';')
            .rsplit([';', '\n'])
            .next()
            .unwrap_or("")
            .trim();
        return (opened_by("try") || opened_by("do"))
            && [
                "throw ",
                "fail(",
                "XCTFail(",
                "Issue.record(",
                "Assert.fail(",
                "Assertions.fail(",
                "assert.fail(",
                "expect.fail(",
            ]
            .iter()
            .any(|marker| last.starts_with(marker));
    }
    let handler_indent = indentation(&code[handler_start..]);
    let mut body = Vec::new();
    for line in before.lines().rev().filter(|line| !line.trim().is_empty()) {
        if indentation(line) > handler_indent {
            body.push(line);
            continue;
        }
        if line.trim() != "try:" {
            return false;
        }
        let base = body.iter().map(|line| indentation(line)).min();
        return body
            .iter()
            .find(|line| Some(indentation(line)) == base)
            .is_some_and(|statement| {
                ["raise", "pytest.fail(", "self.fail(", "assert False"]
                    .iter()
                    .any(|marker| statement.trim_start().starts_with(marker))
            });
    }
    false
}

fn python_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?mR)^[ \t]*except(?:[^\r\n:]*)?:[ \t]*(?:#[^\r\n]*)?\r?\n[ \t]+(?:pass|\.\.\.)[ \t]*(?:#[^\r\n]*)?$")
            .expect("DEAD001 Python regex must compile")
    })
}

fn brace_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?s)catch\s*(?:\([^)]*\))?\s*\{\s*\}")
            .expect("DEAD001 brace regex must compile")
    })
}

/// Swift clauses take a pattern without parentheses: `catch {}`, `catch let error as IOError {}`.
fn swift_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"\bcatch\b[^{};\r\n]*\{\s*\}").expect("DEAD001 Swift regex must compile")
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn detects_empty_python_handler() {
        let source = "try:\n    connect()\nexcept NetworkError:\n    pass\n";
        let context = ScanContext::new(
            Path::new("service.py"),
            source,
            SourceType::Code(Language::Python),
        );
        let mut findings = Vec::new();

        EmptyExceptionHandler.check(&context, &mut findings);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location.line, 3);
    }

    fn count(path: &str, source: &str) -> usize {
        let path = Path::new(path);
        let context = ScanContext::new(path, source, crate::language::classify(path));
        let mut findings = Vec::new();
        EmptyExceptionHandler.check(&context, &mut findings);
        findings.len()
    }

    #[test]
    fn detects_empty_handlers_in_other_catch_languages() {
        for (path, source) in [
            ("Store.java", "try { load(); } catch (IOException e) {}\n"),
            ("Store.kt", "try { load() } catch (e: IOException) {\n}\n"),
            ("Store.cs", "try { Load(); } catch (IOException) { }\n"),
            ("store.cpp", "try { load(); } catch (...) {}\n"),
            ("Store.php", "try { load(); } catch (Exception $e) {}\n"),
            ("Store.swift", "do {\n    try load()\n} catch {}\n"),
            (
                "Store.swift",
                "do {\n    try load()\n} catch let error as IOError {\n}\n",
            ),
            (
                "Store.swift",
                "do {\n    try load()\n} catch is CancellationError {}\n",
            ),
            (
                "Store.swift",
                "do {\n    try load(then: { cache.clear() })\n} catch {}\n",
            ),
        ] {
            assert_eq!(count(path, source), 1, "{source}");
        }
        for (path, source) in [
            (
                "Store.swift",
                "do {\n    try load()\n} catch {\n    // The cache is rebuilt on the next launch.\n}\n",
            ),
            (
                "StoreTests.swift",
                "do {\n    try load()\n    XCTFail(\"Expected a timeout\")\n} catch StoreError.timedOut {\n    // expected\n}\n",
            ),
            (
                "StoreTests.swift",
                "do {\n    _ = try await task.value\n    Issue.record(\"Expected cancellation\")\n} catch is CancellationError {}\n",
            ),
            (
                "StoreTests.swift",
                "do {\n    _ = try fetch(parse: { _ in \"ok\" })\n    Issue.record(\"Expected a timeout\")\n} catch StoreError.timedOut {\n}\n",
            ),
            (
                "StoreTest.java",
                "try {\n    load();\n    fail(\"expected IOException\");\n} catch (IOException expected) {}\n",
            ),
            (
                "Store.kt",
                "fun f(): String = try {\n    read()\n} catch (e: IOException) {\n    \"offline\"\n}\n",
            ),
            ("store.js", "promise.catch(() => {});\n"),
            ("store.go", "func catch() {}\n"),
        ] {
            assert_eq!(count(path, source), 0, "{source}");
        }
    }

    #[test]
    fn parameters_named_ignored_document_the_intent() {
        for (path, source) in [
            (
                "Permissions.java",
                "try { lookup(); } catch (PackageManager.NameNotFoundException ignored) {\n}\n",
            ),
            (
                "Store.kt",
                "try { load() } catch (ignore: IOException) {}\n",
            ),
            ("Store.java", "try { load(); } catch (IOException _) {}\n"),
            (
                "Store.php",
                "try { load(); } catch (Exception $ignored) {}\n",
            ),
        ] {
            assert_eq!(count(path, source), 0, "{source}");
        }
        for (path, source) in [
            (
                "Store.java",
                "try { load(); } catch (IgnoredException e) {}\n",
            ),
            ("Store.java", "try { load(); } catch (IOException e) {}\n"),
        ] {
            assert_eq!(count(path, source), 1, "{source}");
        }
    }

    #[test]
    fn ignores_literal_examples_and_nonempty_python_handlers() {
        for (path, source, language) in [
            (
                "app.js",
                "const example = 'catch (error) {}';\n// catch (error) {}\n",
                Language::JavaScript,
            ),
            (
                "app.py",
                "try:\n    run()\nexcept Exception:\n    pass\n    raise\n",
                Language::Python,
            ),
            (
                "app.py",
                "example = \"\"\"except Exception:\n    pass\n\"\"\"\n",
                Language::Python,
            ),
            (
                "app.js",
                "try { run(); } catch (error) {\n  // Optional cache cleanup.\n}\n",
                Language::JavaScript,
            ),
            (
                "app.py",
                "try:\n    run()\nexcept FileNotFoundError:\n    pass  # Optional cache cleanup.\n",
                Language::Python,
            ),
        ] {
            let context = ScanContext::new(Path::new(path), source, SourceType::Code(language));
            let mut findings = Vec::new();
            EmptyExceptionHandler.check(&context, &mut findings);
            assert!(findings.is_empty(), "{source}");
        }

        let source = "try:\n    run()\nexcept Exception:\n    pass  # TODO\n";
        let context = ScanContext::new(
            Path::new("app.py"),
            source,
            SourceType::Code(Language::Python),
        );
        let mut findings = Vec::new();
        EmptyExceptionHandler.check(&context, &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn permits_optional_imports_exhausted_iterators_and_deliberate_raises() {
        for (path, source, language) in [
            (
                "cli.py",
                "try:\n    import readline\nexcept ImportError:\n    pass\nelse:\n    readline.parse_and_bind(\"tab: complete\")\n",
                Language::Python,
            ),
            (
                "sessions.py",
                "try:\n    first = next(history)\nexcept StopIteration:\n    pass\n",
                Language::Python,
            ),
            (
                "test_app.py",
                "try:\n    raise Exception(\"dummy\")\nexcept Exception:\n    pass\n",
                Language::Python,
            ),
            (
                "test_app.py",
                "try:\n    requests.get(url, timeout=0.1)\n    pytest.fail(\"should time out\")\nexcept ReadTimeout:\n    pass\n",
                Language::Python,
            ),
            (
                "app.test.js",
                "try { throw new Error(\"dummy\"); } catch (error) {}\n",
                Language::JavaScript,
            ),
        ] {
            let context = ScanContext::new(Path::new(path), source, SourceType::Code(language));
            let mut findings = Vec::new();
            EmptyExceptionHandler.check(&context, &mut findings);
            assert!(findings.is_empty(), "{source}");
        }

        let source =
            "try:\n    rows = {}\n    rows.sort(key=index)\nexcept ValueError:\n    pass\n";
        let context = ScanContext::new(
            Path::new("cli.py"),
            source,
            SourceType::Code(Language::Python),
        );
        let mut findings = Vec::new();
        EmptyExceptionHandler.check(&context, &mut findings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn permits_handled_python_exception() {
        let source = "try:\n    connect()\nexcept NetworkError as error:\n    raise RetryError() from error\n";
        let context = ScanContext::new(
            Path::new("service.py"),
            source,
            SourceType::Code(Language::Python),
        );
        let mut findings = Vec::new();

        EmptyExceptionHandler.check(&context, &mut findings);

        assert_eq!(findings, [] as [crate::model::Finding; 0]);
    }
}
