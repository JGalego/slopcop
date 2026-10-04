use std::sync::OnceLock;

use regex::Regex;

use super::{Rule, ScanContext};
use crate::analysis::python_block_ends_at;
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
    false_positives: "A deliberately ignored, narrowly typed exception may be valid when the reason is documented.",
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
            if context.source_type == SourceType::Code(Language::Python)
                && !python_block_ends_at(context.code(), matched.start(), matched.end())
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

fn python_matcher() -> &'static Regex {
    static MATCHER: OnceLock<Regex> = OnceLock::new();
    MATCHER.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*except(?:[^\n:]*)?:[ \t]*(?:#[^\n]*)?\n[ \t]+(?:pass|\.\.\.)[ \t]*(?:#[^\n]*)?$")
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
        ] {
            let context = ScanContext::new(Path::new(path), source, SourceType::Code(language));
            let mut findings = Vec::new();
            EmptyExceptionHandler.check(&context, &mut findings);
            assert!(findings.is_empty(), "{source}");
        }
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
