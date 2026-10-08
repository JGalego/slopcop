//! Sentence vectors and similarity, in integer arithmetic only. A vector is the element-wise sum
//! of its tokens' int8 rows, so there is nothing to round and nothing that differs between CPUs.
//! No floating-point type appears in this module.

use super::model::Model;
use super::tokenizer::tokenize;

/// Texts longer than this many tokens are truncated, as the reference model does.
const MAX_TOKENS: usize = 512;
/// A text with fewer known tokens than this has no vector: too little to compare.
const MIN_KNOWN_TOKENS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vector {
    sums: Vec<i32>,
    norm_squared: i128,
}

impl Vector {
    /// Embeds `text`, or returns `None` when it has too few known tokens.
    #[must_use]
    pub fn new(model: &Model, text: &str) -> Option<Self> {
        Self::from_ids(model, &tokenize(model, text))
    }

    /// Embeds token ids from [`tokenize`].
    #[must_use]
    pub fn from_ids(model: &Model, ids: &[u32]) -> Option<Self> {
        Self::from_ids_min(model, ids, MIN_KNOWN_TOKENS)
    }

    /// Embeds token ids, requiring at least `minimum` known tokens.
    #[must_use]
    pub fn from_ids_min(model: &Model, ids: &[u32], minimum: usize) -> Option<Self> {
        let mut sums = vec![0_i32; model.dims()];
        let mut known = 0;
        for &id in ids.iter().take(MAX_TOKENS) {
            if model.is_special(id) {
                continue;
            }
            known += 1;
            for (sum, &weight) in sums.iter_mut().zip(model.row(id)) {
                *sum += i32::from(weight);
            }
        }
        if known < minimum {
            return None;
        }
        let norm_squared = dot_product(&sums, &sums);
        (norm_squared > 0).then_some(Self { sums, norm_squared })
    }

    /// The components of the vector.
    #[must_use]
    pub fn values(&self) -> &[i32] {
        &self.sums
    }

    /// The number of components.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sums.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sums.is_empty()
    }

    #[must_use]
    pub fn dot(&self, other: &Self) -> i128 {
        dot_product(&self.sums, &other.sums)
    }

    /// The cosine between two vectors in thousandths, rounded toward zero.
    #[must_use]
    pub fn cosine_milli(&self, other: &Self) -> i128 {
        self.dot(other) * 1000 / (self.norm_squared * other.norm_squared).isqrt()
    }
}

fn dot_product(left: &[i32], right: &[i32]) -> i128 {
    left.iter()
        .zip(right)
        .map(|(&a, &b)| i128::from(a) * i128::from(b))
        .sum()
}

/// Whether the cosine between `left` and `right` is at least `numerator / denominator`, decided
/// by squaring both sides: `dot >= 0` and `dot^2 * d^2 >= n^2 * |left|^2 * |right|^2`.
#[must_use]
pub fn at_least(left: &Vector, right: &Vector, numerator: u32, denominator: u32) -> bool {
    let dot = left.dot(right);
    if dot < 0 {
        return false;
    }
    let (n, d) = (i128::from(numerator), i128::from(denominator));
    dot * dot * d * d >= n * n * left.norm_squared * right.norm_squared
}
