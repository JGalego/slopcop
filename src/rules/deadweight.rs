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
    examples: &["except NetworkError:\n    pass", "catch (error) {}"],
    false_positives: "A deliberately ignored exception with an explanatory body comment, an optional import, an exhausted iterator, or a guarded block that always raises is excluded; other undocumented handlers remain findings.",
};

impl Rule for EmptyExceptionHandler {
    fn metadata(&self) -> &'static RuleMetadata {
        &METADATA
    }

    fn check(&self, context: &ScanContext<'_>, findings: &mut Vec<Finding>) {
        let matcher = match context.source_type {
            SourceType::Code(Language::Python) => python_matcher(),
            SourceType::Code(Language::JavaScript | Language::TypeScript) => brace_matcher(),
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

/// `except ImportError: pass` is the idiom for an optional dependency: the import is the probe.
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

/// Reports whether the guarded block always ends by raising: it only throws, or its last statement
/// raises or fails the test. Tests use this shape to put an exception in flight or to expect one,
/// so discarding it is the intended behavior.
fn raises_deliberately(context: &ScanContext<'_>, handler_start: usize) -> bool {
    let code = context.code();
    let before = code[..handler_start].trim_end();
    if context.source_type != SourceType::Code(Language::Python) {
        let Some(open) = before.strip_suffix('}').and_then(|body| body.rfind('{')) else {
            return false;
        };
        let body = &before[..before.len() - 1];
        let statement = body[open + 1..].trim();
        return body[..open].trim_end().ends_with("try")
            && statement.starts_with("throw ")
            && !statement.trim_end_matches(';').contains(';');
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
