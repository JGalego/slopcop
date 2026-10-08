//! The optional polygraph module: rules that need a small, fixed embedding model.
//!
//! Everything here is integer arithmetic over a quantized static model, so a finding is
//! bit-identical on every platform. See `docs/architecture.md`.

mod comments;
mod discover;
mod duplicates;
mod embed;
#[cfg(feature = "polygraph-lm")]
mod lm;
mod model;
mod prep;
#[cfg(feature = "polygraph-lm")]
mod surprisal;
mod tokenizer;

use std::sync::{Arc, OnceLock};

use crate::rules::Rule;

pub use discover::{MODEL_ENV, cached_model_path, load_model, model_file_name};
pub use duplicates::{Paragraph, collect as collect_paragraphs, find as find_duplicates};
pub use embed::{Vector, at_least};
#[cfg(feature = "polygraph-lm")]
pub use lm::{
    LM_ENV, LM_TOKENIZER_SHA256, LM_WEIGHTS_SHA256, LanguageModel, TokenScore,
    installed_language_model, load_language_model,
};
pub use model::{MODEL_SHA256, Model, ModelError, short_hash};
#[cfg(feature = "polygraph-lm")]
pub use surprisal::SentenceScore;
pub use tokenizer::tokenize;

/// Where the model is published. It is a release asset, not part of the crate.
pub const MODEL_URL: &str = "https://github.com/JGalego/slopcop/releases/download/polygraph-model-1/polygraph-ec9c31b3ba4a.bin";

static MODEL: OnceLock<Arc<Model>> = OnceLock::new();

/// Installs the model that polygraph rules use. The first call wins; later calls return the
/// model that is already installed.
pub fn install_model(model: Arc<Model>) -> Arc<Model> {
    Arc::clone(MODEL.get_or_init(|| model))
}

/// Returns the installed model, if any.
#[must_use]
pub fn installed_model() -> Option<&'static Arc<Model>> {
    MODEL.get()
}

/// The polygraph rules that look at one file at a time. `POLY004` compares files, so it runs
/// from [`find_duplicates`] after the scan instead.
pub(crate) fn rules() -> Vec<Box<dyn Rule>> {
    #[allow(unused_mut)]
    let mut rules: Vec<Box<dyn Rule>> = vec![Box::new(comments::CommentRestatesCode)];
    #[cfg(feature = "polygraph-lm")]
    rules.extend([
        Box::new(surprisal::PredictableRun) as Box<dyn Rule>,
        Box::new(surprisal::FlatSurprisal),
    ]);
    rules
}
