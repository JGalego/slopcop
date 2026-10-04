use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Shell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceType {
    Code(Language),
    Documentation,
    Configuration,
    Text,
    Unknown,
}

#[must_use]
pub fn classify(path: &Path) -> SourceType {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    if file_name.eq_ignore_ascii_case("readme") {
        return SourceType::Documentation;
    }

    match extension.to_ascii_lowercase().as_str() {
        "py" | "pyi" => SourceType::Code(Language::Python),
        "js" | "jsx" | "mjs" | "cjs" => SourceType::Code(Language::JavaScript),
        "ts" | "tsx" | "mts" | "cts" => SourceType::Code(Language::TypeScript),
        "go" => SourceType::Code(Language::Go),
        "rs" => SourceType::Code(Language::Rust),
        "java" => SourceType::Code(Language::Java),
        "c" | "h" => SourceType::Code(Language::C),
        "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => SourceType::Code(Language::Cpp),
        "cs" => SourceType::Code(Language::CSharp),
        "rb" => SourceType::Code(Language::Ruby),
        "php" => SourceType::Code(Language::Php),
        "swift" => SourceType::Code(Language::Swift),
        "kt" | "kts" => SourceType::Code(Language::Kotlin),
        "sh" | "bash" | "zsh" | "fish" => SourceType::Code(Language::Shell),
        "md" | "mdx" | "rst" | "adoc" | "asciidoc" => SourceType::Documentation,
        "txt" => SourceType::Text,
        "yaml" | "yml" | "json" | "toml" | "xml" => SourceType::Configuration,
        _ => SourceType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_supported_source_family() {
        let code_extensions = [
            "py", "js", "ts", "go", "rs", "java", "c", "cpp", "cs", "rb", "php", "swift", "kt",
            "sh",
        ];
        for extension in code_extensions {
            assert!(
                matches!(
                    classify(Path::new(&format!("source.{extension}"))),
                    SourceType::Code(_)
                ),
                "{extension} should be code"
            );
        }

        for extension in ["md", "mdx", "rst", "adoc", "asciidoc"] {
            assert_eq!(
                classify(Path::new(&format!("guide.{extension}"))),
                SourceType::Documentation
            );
        }
        for extension in ["yaml", "yml", "json", "toml", "xml"] {
            assert_eq!(
                classify(Path::new(&format!("settings.{extension}"))),
                SourceType::Configuration
            );
        }
    }
}
