use std::collections::HashSet;

use crate::model::Finding;

#[derive(Debug)]
struct Suppression {
    line: usize,
    rules: HashSet<String>,
}

pub fn apply(source: &str, findings: &mut Vec<Finding>) {
    let suppressions = parse(source);
    findings.retain(|finding| {
        !suppressions.iter().any(|suppression| {
            (suppression.line == finding.location.line
                || suppression.line + 1 == finding.location.line)
                && suppression.rules.contains(finding.rule_id)
        })
    });
}

fn parse(source: &str) -> Vec<Suppression> {
    source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let marker = "slopcop: ignore ";
            let start = line.find(marker)? + marker.len();
            let directive = &line[start..];
            let (rule_list, reason) = directive.split_once("--")?;
            if reason.trim().is_empty() {
                return None;
            }
            let rules: HashSet<_> = rule_list
                .split(',')
                .map(str::trim)
                .filter(|rule| is_rule_id(rule))
                .map(str::to_owned)
                .collect();
            (!rules.is_empty()).then_some(Suppression {
                line: index + 1,
                rules,
            })
        })
        .collect()
}

fn is_rule_id(value: &str) -> bool {
    let prefix = value
        .strip_prefix("DEAD")
        .or_else(|| value.strip_prefix("VIBE"));
    prefix
        .is_some_and(|digits| digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::{Confidence, Location, Module, Severity};

    fn finding(line: usize) -> Finding {
        Finding {
            path: PathBuf::from("app.py"),
            location: Location { line, column: 1 },
            rule_id: "DEAD004",
            module: Module::Deadweight,
            severity: Severity::Warning,
            confidence: Confidence::High,
            message: String::new(),
            evidence: None,
            observation: None,
            suggestion: "",
        }
    }

    #[test]
    fn requires_a_reason_and_named_rule() {
        let mut valid = vec![finding(2)];
        apply(
            "# slopcop: ignore DEAD004 -- compatibility probe\nreturn None\n",
            &mut valid,
        );
        assert!(valid.is_empty());

        let mut no_reason = vec![finding(2)];
        apply("# slopcop: ignore DEAD004\nreturn None\n", &mut no_reason);
        assert_eq!(no_reason.len(), 1);

        let mut wildcard = vec![finding(2)];
        apply(
            "# slopcop: ignore all -- hide everything\nreturn None\n",
            &mut wildcard,
        );
        assert_eq!(wildcard.len(), 1);
    }
}
