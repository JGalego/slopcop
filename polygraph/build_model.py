#!/usr/bin/env python3
"""Convert a model2vec static embedding model into the polygraph binary format.

    build_model.py --model-dir DIR --out FILE [--dims 128] [--corpus FILE ...]

DIR holds `model.safetensors` and `tokenizer.json` from minishlab/potion-base-8M at revision
bf8b056651a2c21b8d2565580b8569da283cab23 (MIT). The output is reproducible: the same inputs and
the same dependency versions give an identical file.

Dependencies (tested versions): numpy 2.4.6, safetensors 0.8.0, scipy 1.17.1, tokenizers 0.23.2.

Format, little-endian throughout:

    magic        4 bytes   "SCPG"
    version      u16       1
    dims         u16
    vocab        u32       number of tokens
    specials     u16       number of special token ids, then that many u32 ids
    vocab items  vocab x   u16 byte length, then UTF-8 bytes, in id order
    matrix       vocab x dims int8, row-major

Rows of special tokens are zero. A sentence vector is the element-wise sum of its tokens' rows, so
the model's Zipf weighting is already folded into the rows. Dimensions are reduced by projecting
onto the top right-singular vectors of the uncentered matrix, which keeps sums linear. Rows are
quantized with one global symmetric scale: a per-row scale would change the direction of sums.
"""

import argparse
import hashlib
import json
import re
import struct
import sys
from pathlib import Path

import numpy as np
from safetensors.numpy import load_file
from scipy.stats import spearmanr
from tokenizers import Tokenizer

MAGIC = b"SCPG"
VERSION = 1


def reduce_dims(matrix, dims):
    """Project rows onto the top `dims` right-singular vectors, without centering."""
    if dims >= matrix.shape[1]:
        return matrix
    _, _, vt = np.linalg.svd(matrix.astype(np.float64), full_matrices=False)
    basis = vt[:dims].T
    # Fix each component's sign so the output does not depend on the SVD backend.
    for column in range(basis.shape[1]):
        pivot = np.argmax(np.abs(basis[:, column]))
        if basis[pivot, column] < 0:
            basis[:, column] = -basis[:, column]
    return (matrix.astype(np.float64) @ basis).astype(np.float32)


def quantize(matrix):
    scale = float(np.max(np.abs(matrix))) / 127.0
    return np.clip(np.rint(matrix / scale), -127, 127).astype(np.int8), scale


def write_model(path, vocab, specials, quantized):
    with open(path, "wb") as out:
        out.write(MAGIC)
        out.write(struct.pack("<HHI", VERSION, quantized.shape[1], len(vocab)))
        out.write(struct.pack("<H", len(specials)))
        for identifier in specials:
            out.write(struct.pack("<I", identifier))
        for token in vocab:
            encoded = token.encode("utf-8")
            out.write(struct.pack("<H", len(encoded)))
            out.write(encoded)
        out.write(quantized.tobytes(order="C"))


def sentences(paths):
    found = []
    for path in paths:
        text = Path(path).read_text(encoding="utf-8")
        for sentence in re.split(r"(?<=[.!?])\s+|\n\s*\n", text):
            sentence = " ".join(sentence.split())
            if len(sentence.split()) >= 8:
                found.append(sentence)
    return found


def embed(tokenizer, rows, text):
    ids = tokenizer.encode(text, add_special_tokens=False).ids
    return rows[ids].astype(np.float64).sum(axis=0) if ids else np.zeros(rows.shape[1])


def cosine(a, b):
    norm = np.linalg.norm(a) * np.linalg.norm(b)
    return float(a @ b / norm) if norm else 0.0


def fidelity(tokenizer, original, quantized, corpus):
    pool = sentences(corpus)
    if len(pool) < 200:
        sys.exit(f"need at least 200 corpus sentences for the fidelity check, found {len(pool)}")
    # Adjacent pairs probe near-duplicates; strided pairs probe unrelated text.
    pairs = [(pool[i], pool[i + 1]) for i in range(len(pool) - 1)]
    pairs += [(pool[i], pool[(i * 7 + 13) % len(pool)]) for i in range(len(pool))]
    pairs = pairs[:: max(1, len(pairs) // 400)]
    before = [cosine(embed(tokenizer, original, a), embed(tokenizer, original, b)) for a, b in pairs]
    after = [cosine(embed(tokenizer, quantized, a), embed(tokenizer, quantized, b)) for a, b in pairs]
    return len(pairs), float(spearmanr(before, after).statistic)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--model-dir", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--dims", type=int, default=128)
    parser.add_argument("--corpus", nargs="*", default=[], help="text files for the fidelity check")
    args = parser.parse_args()

    embeddings = load_file(args.model_dir / "model.safetensors")["embeddings"]
    tokenizer_path = args.model_dir / "tokenizer.json"
    tokenizer = Tokenizer.from_file(str(tokenizer_path))
    description = json.loads(tokenizer_path.read_text(encoding="utf-8"))
    vocab_map = description["model"]["vocab"]
    vocab = [None] * len(vocab_map)
    for token, identifier in vocab_map.items():
        vocab[identifier] = token
    if None in vocab or len(vocab) != embeddings.shape[0]:
        sys.exit("vocabulary does not match the embedding matrix")
    specials = sorted(entry["id"] for entry in description["added_tokens"] if entry["special"])

    original = embeddings.copy()
    original[specials] = 0.0
    reduced = reduce_dims(original, args.dims)
    quantized, scale = quantize(reduced)
    write_model(args.out, vocab, specials, quantized)

    digest = hashlib.sha256(args.out.read_bytes()).hexdigest()
    print(f"wrote {args.out}: {args.out.stat().st_size} bytes, dims {quantized.shape[1]}, scale {scale:.6g}")
    print(f"sha256 {digest}")
    if args.corpus:
        count, rho = fidelity(tokenizer, original, quantized.astype(np.float32), args.corpus)
        print(f"fidelity: Spearman {rho:.4f} over {count} sentence pairs")
        if rho < 0.98:
            sys.exit("fidelity below 0.98")


if __name__ == "__main__":
    main()
