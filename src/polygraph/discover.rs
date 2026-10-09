use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::model::{MODEL_SHA256, Model, short_hash};
use super::{MODEL_URL, install_model};

/// Name of the environment variable that points at the model file.
pub const MODEL_ENV: &str = "SLOPCOP_POLYGRAPH_MODEL";

/// The file name of the model in a cache directory and in the release it comes from.
#[must_use]
pub fn model_file_name() -> String {
    format!("polygraph-{}.bin", short_hash(MODEL_SHA256))
}

/// Where a downloaded model is looked up when nothing else names one.
#[must_use]
pub fn cached_model_path() -> Option<PathBuf> {
    let cache = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from))?;
    Some(cache.join("slopcop").join(model_file_name()))
}

/// Finds, verifies, and installs the model. Sources are tried in this order, and the first one
/// that names a path is the only one used: the `explicit` command-line path, the environment
/// variable, the `configured` path, and the cache directory.
///
/// # Errors
///
/// Returns a message, ready to print, when no model is named, the file cannot be read, or it is
/// not the model this build accepts.
pub fn load_model(
    explicit: Option<&Path>,
    configured: Option<&Path>,
) -> Result<Arc<Model>, String> {
    let from_env = env::var_os(MODEL_ENV).map(PathBuf::from);
    let path = explicit
        .map(Path::to_path_buf)
        .or(from_env)
        .or_else(|| configured.map(Path::to_path_buf))
        .or_else(|| cached_model_path().filter(|path| path.is_file()))
        .ok_or_else(missing_model)?;
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "could not read the polygraph model {}: {error}",
            path.display()
        )
    })?;
    let model = Model::from_bytes(&bytes)
        .map_err(|error| format!("polygraph model {}: {error}", path.display()))?;
    Ok(install_model(Arc::new(model)))
}

fn missing_model() -> String {
    let target =
        cached_model_path().map_or_else(model_file_name, |path| path.display().to_string());
    format!(
        "polygraph is enabled but no model file was found.\n\
         Download it and check its SHA-256:\n  \
         curl -fL --create-dirs -o {target} {MODEL_URL}\n  \
         echo \"{MODEL_SHA256}  {target}\" | sha256sum -c\n\
         or name a model with --polygraph-model, {MODEL_ENV}, or `model` under [slopcop.polygraph]."
    )
}
