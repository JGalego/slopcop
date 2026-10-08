use std::collections::HashMap;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// SHA-256 of the only model file this build accepts.
pub const MODEL_SHA256: &str = "ec9c31b3ba4ad39c9856f91484b883316078b35215e3b4e0ae04e4c797880a93";

const MAGIC: &[u8; 4] = b"SCPG";
const VERSION: u16 = 1;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("model file has SHA-256 {actual}, expected {expected}")]
    Hash {
        actual: String,
        expected: &'static str,
    },
    #[error("model file is malformed: {0}")]
    Format(&'static str),
}

/// A quantized static embedding model: a vocabulary and one int8 row per token.
#[derive(Debug)]
pub struct Model {
    dims: usize,
    vocab: HashMap<Box<str>, u32>,
    specials: Vec<bool>,
    special_tokens: Vec<(Box<str>, u32)>,
    continuations: Vec<bool>,
    unknown: u32,
    rows: Vec<i8>,
}

/// The first twelve hex digits of a SHA-256, used in file names and finding evidence.
#[must_use]
pub fn short_hash(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

impl Model {
    /// Parses and verifies a model file.
    ///
    /// # Errors
    ///
    /// Returns an error when the SHA-256 differs from [`MODEL_SHA256`] or the layout is invalid.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
        let actual = hex(&Sha256::digest(bytes));
        if actual != MODEL_SHA256 {
            return Err(ModelError::Hash {
                actual,
                expected: MODEL_SHA256,
            });
        }
        Self::parse(bytes)
    }

    fn parse(bytes: &[u8]) -> Result<Self, ModelError> {
        let mut reader = Reader(bytes);
        if reader.take(4)? != MAGIC {
            return Err(ModelError::Format("bad magic"));
        }
        if reader.u16()? != VERSION {
            return Err(ModelError::Format("unsupported version"));
        }
        let dims = usize::from(reader.u16()?);
        let count = reader.u32()? as usize;
        if dims == 0 || count == 0 {
            return Err(ModelError::Format("empty model"));
        }
        let mut specials = vec![false; count];
        let mut special_ids = Vec::new();
        for _ in 0..reader.u16()? {
            let id = reader.u32()? as usize;
            *specials
                .get_mut(id)
                .ok_or(ModelError::Format("special id out of range"))? = true;
            special_ids.push(id);
        }
        let mut vocab: HashMap<Box<str>, u32> = HashMap::with_capacity(count);
        for id in 0..count {
            let length = usize::from(reader.u16()?);
            let token = std::str::from_utf8(reader.take(length)?)
                .map_err(|_| ModelError::Format("token is not UTF-8"))?;
            vocab.insert(Box::from(token), u32::try_from(id).expect("count fits u32"));
        }
        let matrix = reader.take(count * dims)?;
        if !reader.0.is_empty() {
            return Err(ModelError::Format("trailing bytes"));
        }
        let special_tokens = special_ids
            .iter()
            .filter_map(|&id| {
                let id = u32::try_from(id).ok()?;
                vocab
                    .iter()
                    .find(|&(_, &candidate)| candidate == id)
                    .map(|(token, _)| (token.clone(), id))
            })
            .collect();
        let mut continuations = vec![false; count];
        for (token, &id) in &vocab {
            continuations[id as usize] = token.starts_with("##");
        }
        let unknown = *vocab
            .get("[UNK]")
            .ok_or(ModelError::Format("no [UNK] token"))?;
        Ok(Self {
            dims,
            vocab,
            specials,
            special_tokens,
            continuations,
            unknown,
            rows: matrix
                .iter()
                .map(|&byte| i8::from_le_bytes([byte]))
                .collect(),
        })
    }

    #[must_use]
    pub fn dims(&self) -> usize {
        self.dims
    }

    pub(super) fn id(&self, token: &str) -> Option<u32> {
        self.vocab.get(token).copied()
    }

    pub(super) fn special_tokens(&self) -> &[(Box<str>, u32)] {
        &self.special_tokens
    }

    /// Whether a token continues a word, as `##ing` does.
    pub(super) fn is_continuation(&self, id: u32) -> bool {
        self.continuations
            .get(id as usize)
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn unknown(&self) -> u32 {
        self.unknown
    }

    pub(super) fn is_special(&self, id: u32) -> bool {
        self.specials.get(id as usize).copied().unwrap_or(true)
    }

    pub(super) fn row(&self, id: u32) -> &[i8] {
        let start = id as usize * self.dims;
        &self.rows[start..start + self.dims]
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], ModelError> {
        if self.0.len() < length {
            return Err(ModelError::Format("unexpected end of file"));
        }
        let (head, tail) = self.0.split_at(length);
        self.0 = tail;
        Ok(head)
    }

    fn u16(&mut self) -> Result<u16, ModelError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, ModelError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}
