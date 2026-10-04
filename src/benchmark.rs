use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::scanner::{ScanOptions, SourceFile, scan_sources};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CorpusKind {
    Mixed,
    Documentation,
    Code,
}

#[derive(Clone, Copy, Debug)]
struct CorpusSpec {
    name: &'static str,
    files: usize,
    kind: CorpusKind,
}

const CORPORA: &[CorpusSpec] = &[
    CorpusSpec {
        name: "tiny repo",
        files: 32,
        kind: CorpusKind::Mixed,
    },
    CorpusSpec {
        name: "medium repo",
        files: 512,
        kind: CorpusKind::Mixed,
    },
    CorpusSpec {
        name: "large repo",
        files: 4_096,
        kind: CorpusKind::Mixed,
    },
    CorpusSpec {
        name: "docs-heavy",
        files: 1_024,
        kind: CorpusKind::Documentation,
    },
    CorpusSpec {
        name: "code-heavy",
        files: 1_024,
        kind: CorpusKind::Code,
    },
];

#[derive(Clone, Debug)]
pub struct Measurement {
    pub name: &'static str,
    pub files: u32,
    pub bytes: u32,
    pub findings: usize,
    pub elapsed: Duration,
}

impl Measurement {
    #[must_use]
    pub fn files_per_second(&self) -> f64 {
        f64::from(self.files) / self.elapsed.as_secs_f64().max(f64::EPSILON)
    }

    #[must_use]
    pub fn megabytes_per_second(&self) -> f64 {
        (f64::from(self.bytes) / 1_000_000.0) / self.elapsed.as_secs_f64().max(f64::EPSILON)
    }
}

#[must_use]
pub fn run() -> Vec<Measurement> {
    let options = ScanOptions {
        max_file_size: 1_000_000,
        config: Config::default(),
    };
    let _ = scan_sources(
        vec![code_source(usize::MAX), documentation_source(usize::MAX)],
        &options,
    );
    CORPORA
        .iter()
        .map(|spec| measure(*spec, &options))
        .collect()
}

fn measure(spec: CorpusSpec, options: &ScanOptions) -> Measurement {
    const SAMPLES: usize = 3;
    let bytes: usize = corpus(spec).iter().map(|source| source.bytes.len()).sum();
    let mut elapsed = Vec::with_capacity(SAMPLES);
    let mut findings = 0;
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let result = scan_sources(corpus(spec), options);
        elapsed.push(started.elapsed());
        findings = result.findings.len();
    }
    elapsed.sort_unstable();
    Measurement {
        name: spec.name,
        files: spec
            .files
            .try_into()
            .expect("benchmark corpus file count must fit in u32"),
        bytes: bytes
            .try_into()
            .expect("benchmark corpus byte count must fit in u32"),
        findings,
        elapsed: elapsed[SAMPLES / 2],
    }
}

fn corpus(spec: CorpusSpec) -> Vec<SourceFile> {
    (0..spec.files)
        .map(|index| match (spec.kind, index % 3) {
            (CorpusKind::Documentation, _) | (CorpusKind::Mixed, 0) => documentation_source(index),
            (CorpusKind::Code | CorpusKind::Mixed, _) => code_source(index),
        })
        .collect()
}

fn code_source(index: usize) -> SourceFile {
    let source = format!(
        "use std::io;\n\npub fn parse_record_{index}(input: &[u8]) -> io::Result<usize> {{\n    let length = input.len();\n    if length > 1_000_000 {{\n        return Err(io::Error::new(io::ErrorKind::InvalidData, \"record too large\"));\n    }}\n    Ok(length)\n}}\n\npub fn write_record_{index}(length: usize) -> String {{\n    format!(\"record-{index}-{{length}}\")\n}}\n"
    );
    SourceFile {
        path: PathBuf::from(format!("src/module_{index}.rs")),
        bytes: source.into_bytes(),
    }
}

fn documentation_source(index: usize) -> SourceFile {
    let source = format!(
        "# Parser {index}\n\nParser {index} reads one framed record and returns its byte length. It rejects inputs larger than one megabyte before allocating output.\n\n## Failure modes\n\nMalformed lengths return `InvalidData`. I/O failures retain their original error kind and source.\n\n## Example\n\nCall `parse_record_{index}` with a borrowed byte slice. Check the returned length before writing the response.\n"
    );
    SourceFile {
        path: PathBuf::from(format!("docs/parser_{index}.md")),
        bytes: source.into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_corpora_are_stable() {
        assert_eq!(CORPORA.len(), 5);
        assert_eq!(CORPORA[0].name, "tiny repo");
        let sample = corpus(CorpusSpec {
            name: "sample",
            files: 6,
            kind: CorpusKind::Mixed,
        });
        assert_eq!(sample.len(), 6);
        assert!(
            sample
                .iter()
                .any(|source| source.path.extension().is_some_and(|value| value == "md"))
        );
        assert!(
            sample
                .iter()
                .any(|source| source.path.extension().is_some_and(|value| value == "rs"))
        );
    }
}
