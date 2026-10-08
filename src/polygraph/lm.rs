//! A small causal language model, SmolLM2-135M, run on the CPU with candle to score how
//! predictable each token of a text is. Scores are `f32` and are reproducible on one build and CPU
//! family; they are not bit-identical across CPUs, which is why the rules built on them are
//! informational.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_nn::ops::{log_softmax, softmax_last_dim};
use sha2::{Digest, Sha256};
use tokenizers::Tokenizer;

/// SHA-256 of `model.safetensors` and `tokenizer.json` from HuggingFaceTB/SmolLM2-135M at
/// revision 93efa2f097d58c2a74874c7e644dbc9b0cee75a2, the only files this build accepts.
pub const LM_WEIGHTS_SHA256: &str =
    "80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1";
pub const LM_TOKENIZER_SHA256: &str =
    "9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c";

/// Name of the environment variable that points at the language-model directory.
pub const LM_ENV: &str = "SLOPCOP_POLYGRAPH_LM";

const HIDDEN: usize = 576;
const HEADS: usize = 9;
const KV_HEADS: usize = 3;
const HEAD_DIM: usize = HIDDEN / HEADS;
const LAYERS: usize = 30;
const ROPE_THETA: f32 = 100_000.0;
const RMS_EPS: f32 = 1e-5;
/// The end-of-text token, which starts every window.
const START_TOKEN: u32 = 0;
/// Tokens per forward pass, not counting the start token.
const WINDOW: usize = 256;

/// The surprisal of one token, in bits, with its byte range in the scored text.
#[derive(Clone, Copy, Debug)]
pub struct TokenScore {
    pub start: usize,
    pub end: usize,
    pub bits: f32,
    /// Whether the token opens a window and so had only the start token as context.
    pub first_in_window: bool,
}

struct Layer {
    input_norm: Tensor,
    post_norm: Tensor,
    q: Tensor,
    k: Tensor,
    v: Tensor,
    o: Tensor,
    gate: Tensor,
    up: Tensor,
    down: Tensor,
}

/// The weights and tables, owned by the inference thread.
struct Network {
    embeddings: Tensor,
    layers: Vec<Layer>,
    final_norm: Tensor,
    cos: Tensor,
    sin: Tensor,
    device: Device,
}

/// One window of token ids to score, and where to send the surprisals.
struct Request {
    ids: Vec<u32>,
    reply: mpsc::Sender<Result<Vec<f32>, String>>,
}

/// Inference runs on one dedicated thread with a rayon pool of its own, and callers wait on a
/// channel. A caller must not wait on a lock or on a rayon call into another pool: a scanner
/// worker that does either steals another file's job while it waits, and that job needs the same
/// model, which deadlocks.
pub struct LanguageModel {
    tokenizer: Tokenizer,
    requests: Mutex<mpsc::Sender<Request>>,
}

static LM: OnceLock<Arc<LanguageModel>> = OnceLock::new();

/// Installs the language model that `POLY001` and `POLY002` use. The first call wins.
pub fn install_language_model(model: Arc<LanguageModel>) -> Arc<LanguageModel> {
    Arc::clone(LM.get_or_init(|| model))
}

#[must_use]
pub fn installed_language_model() -> Option<&'static Arc<LanguageModel>> {
    LM.get()
}

/// Reads, verifies, and installs the language model in `directory`, or in the directory that
/// [`LM_ENV`] names when `directory` is `None`.
///
/// # Errors
///
/// Returns a message, ready to print, when the files are missing, do not match the pinned
/// SHA-256, or cannot be loaded.
pub fn load_language_model(directory: Option<&Path>) -> Result<Arc<LanguageModel>, String> {
    let from_env = std::env::var_os(LM_ENV).map(std::path::PathBuf::from);
    let directory = directory
        .map(Path::to_path_buf)
        .or(from_env)
        .ok_or_else(|| {
            format!(
                "the polygraph language model needs a directory with model.safetensors and tokenizer.json \
                 from HuggingFaceTB/SmolLM2-135M at revision 93efa2f097d58c2a74874c7e644dbc9b0cee75a2; \
                 name it with --polygraph-lm, {LM_ENV}, or `language-model` under [slopcop.polygraph]"
            )
        })?;
    let read = |name: &str, expected: &str| -> Result<Vec<u8>, String> {
        let path = directory.join(name);
        let bytes = fs::read(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let actual = hex(&Sha256::digest(&bytes));
        if actual == expected {
            Ok(bytes)
        } else {
            Err(format!(
                "{} has SHA-256 {actual}, expected {expected}",
                path.display()
            ))
        }
    };
    let weights = read("model.safetensors", LM_WEIGHTS_SHA256)?;
    let tokenizer = read("tokenizer.json", LM_TOKENIZER_SHA256)?;
    let model = LanguageModel::from_bytes(&weights, &tokenizer)
        .map_err(|error| format!("could not load the polygraph language model: {error}"))?;
    Ok(install_language_model(Arc::new(model)))
}

impl LanguageModel {
    fn from_bytes(weights: &[u8], tokenizer: &[u8]) -> candle_core::Result<Self> {
        let device = Device::Cpu;
        let tokenizer = Tokenizer::from_bytes(tokenizer)
            .map_err(|error| candle_core::Error::Msg(error.to_string()))?;
        let vb = VarBuilder::from_slice_safetensors(weights, DType::F32, &device)?;
        let model = vb.pp("model");
        let embeddings = model.pp("embed_tokens").get((49_152, HIDDEN), "weight")?;
        let mut layers = Vec::with_capacity(LAYERS);
        for index in 0..LAYERS {
            let layer = model.pp(format!("layers.{index}"));
            let attention = layer.pp("self_attn");
            let mlp = layer.pp("mlp");
            let kv = KV_HEADS * HEAD_DIM;
            layers.push(Layer {
                input_norm: layer.pp("input_layernorm").get(HIDDEN, "weight")?,
                post_norm: layer.pp("post_attention_layernorm").get(HIDDEN, "weight")?,
                q: attention
                    .pp("q_proj")
                    .get((HIDDEN, HIDDEN), "weight")?
                    .t()?
                    .contiguous()?,
                k: attention
                    .pp("k_proj")
                    .get((kv, HIDDEN), "weight")?
                    .t()?
                    .contiguous()?,
                v: attention
                    .pp("v_proj")
                    .get((kv, HIDDEN), "weight")?
                    .t()?
                    .contiguous()?,
                o: attention
                    .pp("o_proj")
                    .get((HIDDEN, HIDDEN), "weight")?
                    .t()?
                    .contiguous()?,
                gate: mlp
                    .pp("gate_proj")
                    .get((1536, HIDDEN), "weight")?
                    .t()?
                    .contiguous()?,
                up: mlp
                    .pp("up_proj")
                    .get((1536, HIDDEN), "weight")?
                    .t()?
                    .contiguous()?,
                down: mlp
                    .pp("down_proj")
                    .get((HIDDEN, 1536), "weight")?
                    .t()?
                    .contiguous()?,
            });
        }
        let final_norm = model.pp("norm").get(HIDDEN, "weight")?;
        let (cos, sin) = rotary_tables(WINDOW + 1, &device)?;
        let network = Network {
            embeddings,
            layers,
            final_norm,
            cos,
            sin,
            device,
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .build()
            .map_err(|error| candle_core::Error::Msg(error.to_string()))?;
        let (requests, queue) = mpsc::channel::<Request>();
        std::thread::Builder::new()
            .name("polygraph-lm".to_owned())
            .spawn(move || {
                for request in queue {
                    let bits = pool
                        .install(|| network.window_bits(&request.ids))
                        .map_err(|error| error.to_string());
                    // The caller may have given up; its reply channel is then closed.
                    let _ = request.reply.send(bits);
                }
            })
            .map_err(|error| candle_core::Error::Msg(error.to_string()))?;
        Ok(Self {
            tokenizer,
            requests: Mutex::new(requests),
        })
    }

    /// The surprisal of every token of `text`, up to `max_tokens` tokens. The text is scored in
    /// windows, each led by the start token.
    ///
    /// # Errors
    ///
    /// Returns a message when tokenizing or the forward pass fails.
    pub fn score(&self, text: &str, max_tokens: usize) -> Result<Vec<TokenScore>, String> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|error| error.to_string())?;
        let ids = encoding.get_ids();
        let offsets = encoding.get_offsets();
        let count = ids.len().min(max_tokens);
        let mut scores = Vec::with_capacity(count);
        for window in (0..count).step_by(WINDOW) {
            let end = (window + WINDOW).min(count);
            let (reply, answer) = mpsc::channel();
            self.requests
                .lock()
                .map_err(|error| error.to_string())?
                .send(Request {
                    ids: ids[window..end].to_vec(),
                    reply,
                })
                .map_err(|error| error.to_string())?;
            let bits = answer.recv().map_err(|error| error.to_string())??;
            for (position, bits) in bits.into_iter().enumerate() {
                let (start, stop) = offsets[window + position];
                scores.push(TokenScore {
                    start,
                    end: stop,
                    bits,
                    first_in_window: position == 0,
                });
            }
        }
        Ok(scores)
    }
}

impl Network {
    fn window_bits(&self, ids: &[u32]) -> candle_core::Result<Vec<f32>> {
        let mut input = Vec::with_capacity(ids.len() + 1);
        input.push(START_TOKEN);
        input.extend_from_slice(ids);
        let length = input.len();
        let tokens = Tensor::new(input.as_slice(), &self.device)?;
        let mut x = self.embeddings.embedding(&tokens)?.unsqueeze(0)?;
        let mask = causal_mask(length, &self.device)?;
        let cos = self.cos.narrow(0, 0, length)?;
        let sin = self.sin.narrow(0, 0, length)?;
        for layer in &self.layers {
            let h = rms_norm(&x, &layer.input_norm)?;
            let attention = Self::attention(&h, layer, &cos, &sin, &mask)?;
            x = (x + attention)?;
            let h = rms_norm(&x, &layer.post_norm)?;
            let gated = (candle_nn::ops::silu(&h.broadcast_matmul(&layer.gate)?)?
                * h.broadcast_matmul(&layer.up)?)?;
            x = (x + gated.broadcast_matmul(&layer.down)?)?;
        }
        let x = rms_norm(&x, &self.final_norm)?;
        // Position `i` predicts token `i + 1`; the last position predicts nothing we score.
        let logits = x.i((0, ..length - 1))?.matmul(&self.embeddings.t()?)?;
        let log_probs = log_softmax(&logits, D::Minus1)?;
        let targets = Tensor::new(ids, &self.device)?.unsqueeze(1)?;
        let chosen = log_probs.gather(&targets, 1)?.squeeze(1)?;
        let bits = (chosen.neg()? / std::f64::consts::LN_2)?;
        bits.to_vec1::<f32>()
    }

    fn attention(
        h: &Tensor,
        layer: &Layer,
        cos: &Tensor,
        sin: &Tensor,
        mask: &Tensor,
    ) -> candle_core::Result<Tensor> {
        let (batch, length, _) = h.dims3()?;
        let split = |t: Tensor, heads: usize| -> candle_core::Result<Tensor> {
            t.reshape((batch, length, heads, HEAD_DIM))?
                .transpose(1, 2)?
                .contiguous()
        };
        let q = split(h.broadcast_matmul(&layer.q)?, HEADS)?;
        let k = split(h.broadcast_matmul(&layer.k)?, KV_HEADS)?;
        let v = split(h.broadcast_matmul(&layer.v)?, KV_HEADS)?;
        let q = candle_nn::rotary_emb::rope(&q, cos, sin)?;
        let k = candle_nn::rotary_emb::rope(&k, cos, sin)?;
        let repeat = HEADS / KV_HEADS;
        let expand = |t: Tensor| -> candle_core::Result<Tensor> {
            t.unsqueeze(2)?
                .expand((batch, KV_HEADS, repeat, length, HEAD_DIM))?
                .reshape((batch, HEADS, length, HEAD_DIM))
        };
        let (k, v) = (expand(k)?, expand(v)?);
        #[allow(clippy::cast_precision_loss)]
        let scale = 1.0 / (HEAD_DIM as f64).sqrt();
        let scores = (q.matmul(&k.transpose(2, 3)?.contiguous()?)? * scale)?;
        let weights = softmax_last_dim(&scores.broadcast_add(mask)?)?;
        let context = weights.matmul(&v.contiguous()?)?;
        context
            .transpose(1, 2)?
            .reshape((batch, length, HIDDEN))?
            .broadcast_matmul(&layer.o)
    }
}

fn rms_norm(x: &Tensor, weight: &Tensor) -> candle_core::Result<Tensor> {
    candle_nn::ops::rms_norm(&x.contiguous()?, weight, RMS_EPS)
}

fn causal_mask(length: usize, device: &Device) -> candle_core::Result<Tensor> {
    let values: Vec<f32> = (0..length)
        .flat_map(|row| {
            (0..length).map(move |column| if column > row { f32::NEG_INFINITY } else { 0.0 })
        })
        .collect();
    Tensor::from_vec(values, (length, length), device)
}

fn rotary_tables(length: usize, device: &Device) -> candle_core::Result<(Tensor, Tensor)> {
    let half = HEAD_DIM / 2;
    #[allow(clippy::cast_precision_loss)]
    let inverse: Vec<f32> = (0..half)
        .map(|index| 1.0 / ROPE_THETA.powf(2.0 * index as f32 / HEAD_DIM as f32))
        .collect();
    let mut cos = Vec::with_capacity(length * half);
    let mut sin = Vec::with_capacity(length * half);
    for position in 0..length {
        for frequency in &inverse {
            #[allow(clippy::cast_precision_loss)]
            let angle = position as f32 * frequency;
            cos.push(angle.cos());
            sin.push(angle.sin());
        }
    }
    Ok((
        Tensor::from_vec(cos, (length, half), device)?,
        Tensor::from_vec(sin, (length, half), device)?,
    ))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}
