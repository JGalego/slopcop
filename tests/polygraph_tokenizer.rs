#![cfg(feature = "polygraph")]

//! Pins the Rust tokenizer and integer pipeline to the reference implementation. Needs the model
//! file: set `SLOPCOP_POLYGRAPH_MODEL` or run `make polygraph-model`.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use slopcop::polygraph::{Model, Vector, cached_model_path, tokenize};

fn model() -> Model {
    let path = std::env::var_os("SLOPCOP_POLYGRAPH_MODEL")
        .map(PathBuf::from)
        .or_else(|| cached_model_path().filter(|path| path.is_file()))
        .unwrap_or_else(|| panic!("set SLOPCOP_POLYGRAPH_MODEL to the polygraph model file; `make polygraph-model` downloads it"));
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    Model::from_bytes(&bytes).expect("valid model")
}

fn golden(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/polygraph")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).expect("golden file")).expect("golden JSON")
}

#[test]
fn tokenizer_matches_reference() {
    let model = model();
    let cases = golden("tokenizer.json");
    assert!(
        cases.len() >= 500,
        "golden file must keep at least 500 cases"
    );
    let mut failures = Vec::new();
    for case in &cases {
        let text = case[0].as_str().expect("text");
        let expected: Vec<u32> = case[1]
            .as_array()
            .expect("ids")
            .iter()
            .map(|id| u32::try_from(id.as_u64().expect("id")).expect("id fits"))
            .collect();
        let actual = tokenize(&model, text);
        if actual != expected {
            failures.push(format!(
                "{text:?}\n  expected {expected:?}\n  actual   {actual:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn embeddings_are_bit_exact() {
    let model = model();
    for case in golden("embeddings.json") {
        let text = |index: usize| case[index].as_str().expect("text");
        let number = |index: usize| text(index).parse::<i128>().expect("integer");
        let first = Vector::new(&model, text(0)).expect("first vector");
        let second = Vector::new(&model, text(1)).expect("second vector");
        assert_eq!(
            first.dot(&second),
            number(2),
            "dot product of {:?}",
            text(0)
        );
        assert_eq!(first.dot(&first), number(3), "norm of {:?}", text(0));
        assert_eq!(second.dot(&second), number(4), "norm of {:?}", text(1));
    }
}
