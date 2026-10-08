#![cfg(feature = "polygraph-lm")]

//! Pins the Rust language-model forward pass to the reference implementation. Needs the model
//! directory: set `SLOPCOP_POLYGRAPH_LM` to a directory with `model.safetensors` and
//! `tokenizer.json` from HuggingFaceTB/SmolLM2-135M.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use slopcop::polygraph::load_language_model;

#[test]
fn surprisals_match_the_reference_within_tolerance() {
    let model = load_language_model(None).unwrap_or_else(|message| panic!("{message}"));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/polygraph/lm.json");
    let cases: Vec<Value> =
        serde_json::from_str(&fs::read_to_string(path).expect("golden file")).expect("golden JSON");
    for case in &cases {
        let text = case["text"].as_str().expect("text");
        let expected: Vec<f64> = case["bits"]
            .as_array()
            .expect("bits")
            .iter()
            .map(|bits| bits.as_f64().expect("number"))
            .collect();
        let scores = model.score(text, 256).expect("scores");
        assert_eq!(scores.len(), expected.len(), "token count of {text:?}");
        for (index, (score, expected)) in scores.iter().zip(&expected).enumerate() {
            assert!(
                (f64::from(score.bits) - expected).abs() < 0.02,
                "token {index} of {text:?}: {} bits, reference {expected}",
                score.bits
            );
        }
    }
}
