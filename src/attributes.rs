//! Reads the `linguist-vendored` and `linguist-generated` attributes that repositories set in
//! `.gitattributes` to tell GitHub which files are third-party or generated. Discovery leaves those
//! files out, because their problems belong to the upstream project or the generator.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

pub const ATTRIBUTES_FILE_NAME: &str = ".gitattributes";

const ATTRIBUTES: [&str; 2] = ["linguist-vendored", "linguist-generated"];

/// The vendored and generated patterns of every `.gitattributes` file read so far.
#[derive(Clone, Debug, Default)]
pub struct LinguistExclusions {
    /// Matchers keyed by the directory holding their `.gitattributes`, one per attribute.
    files: BTreeMap<PathBuf, [Gitignore; 2]>,
}

impl LinguistExclusions {
    /// Adds the attributes declared by the `.gitattributes` file in `directory`.
    pub fn add(&mut self, directory: &Path, contents: &str) {
        let mut builders = ATTRIBUTES.map(|_| GitignoreBuilder::new(directory));
        for line in contents.lines() {
            let mut fields = line.split_whitespace();
            let Some(pattern) = fields.next() else {
                continue;
            };
            // Comments, macro definitions, and quoted patterns carry no linguist settings that
            // need support here; a leading `!` is not a valid attribute pattern.
            if pattern.starts_with(['#', '[', '"', '!']) {
                continue;
            }
            for field in fields {
                for (attribute, builder) in ATTRIBUTES.iter().zip(&mut builders) {
                    let line = match setting(field, attribute) {
                        Some(true) => pattern.to_owned(),
                        Some(false) => format!("!{pattern}"),
                        None => continue,
                    };
                    let _ = builder.add_line(None, &line);
                }
            }
        }
        let matchers =
            builders.map(|builder| builder.build().unwrap_or_else(|_| Gitignore::empty()));
        if matchers.iter().any(|matcher| !matcher.is_empty()) {
            self.files.insert(directory.to_path_buf(), matchers);
        }
    }

    /// Whether the nearest `.gitattributes` that mentions a file marks it vendored or generated.
    /// Later lines override earlier ones and deeper files override shallower ones, as in Git.
    #[must_use]
    pub fn excludes(&self, path: &Path) -> bool {
        (0..ATTRIBUTES.len()).any(|attribute| {
            self.files
                .iter()
                .rev()
                .filter(|(directory, _)| path.starts_with(directory))
                .find_map(
                    |(_, matchers)| match matchers[attribute].matched(path, false) {
                        Match::None => None,
                        Match::Ignore(_) => Some(true),
                        Match::Whitelist(_) => Some(false),
                    },
                )
                .unwrap_or(false)
        })
    }
}

/// Reads one attribute field: `attr` and `attr=value` set it, while `-attr` and `attr=false` unset
/// it, matching how GitHub Linguist interprets these attributes.
fn setting(field: &str, attribute: &str) -> Option<bool> {
    if let Some(name) = field.strip_prefix('-') {
        return (name == attribute).then_some(false);
    }
    match field.split_once('=') {
        Some((name, value)) => (name == attribute).then_some(value != "false"),
        None => (field == attribute).then_some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exclusions(files: &[(&str, &str)]) -> LinguistExclusions {
        let mut exclusions = LinguistExclusions::default();
        for (directory, contents) in files {
            exclusions.add(Path::new(directory), contents);
        }
        exclusions
    }

    #[test]
    fn vendored_and_generated_paths_are_excluded() {
        let exclusions = exclusions(&[(
            "/repo",
            "# third-party\nlib/libc/** linguist-vendored\n*.pb.go linguist-generated=true\n*.c text eol=lf\n",
        )]);
        assert!(exclusions.excludes(Path::new("/repo/lib/libc/musl/stdio.c")));
        assert!(exclusions.excludes(Path::new("/repo/api/service.pb.go")));
        assert!(!exclusions.excludes(Path::new("/repo/src/main.c")));
        assert!(!exclusions.excludes(Path::new("/elsewhere/lib/libc/stdio.c")));
    }

    #[test]
    fn later_lines_and_deeper_files_override_earlier_settings() {
        let exclusions = exclusions(&[
            (
                "/repo",
                "deps/** linguist-vendored\ndeps/ours/** -linguist-vendored\nweb/** linguist-generated\n",
            ),
            ("/repo/web", "src/** linguist-generated=false\n"),
        ]);
        assert!(exclusions.excludes(Path::new("/repo/deps/zlib/inflate.c")));
        assert!(!exclusions.excludes(Path::new("/repo/deps/ours/shim.c")));
        assert!(exclusions.excludes(Path::new("/repo/web/dist/app.js")));
        assert!(!exclusions.excludes(Path::new("/repo/web/src/app.js")));
    }

    #[test]
    fn patterns_follow_attribute_matching_rather_than_directory_inheritance() {
        // In Git, `dir/*` assigns the attribute to direct children only, not to deeper files.
        let exclusions = exclusions(&[("/repo", "res/* linguist-vendored\n")]);
        assert!(exclusions.excludes(Path::new("/repo/res/font.ttf")));
        assert!(!exclusions.excludes(Path::new("/repo/res/icons/README.md")));
    }

    #[test]
    fn relative_roots_match_repository_relative_paths() {
        let exclusions = exclusions(&[("", "tests/wpt/** linguist-vendored\n")]);
        assert!(exclusions.excludes(Path::new("tests/wpt/dom/events.js")));
        assert!(!exclusions.excludes(Path::new("tests/unit/dom.rs")));
    }
}
