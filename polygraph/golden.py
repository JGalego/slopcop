#!/usr/bin/env python3
"""Write the golden files that pin the polygraph tokenizer and integer pipeline.

    golden.py --model-dir DIR --model FILE --out tests/polygraph

`tokenizer.json` holds `[text, token_ids]` pairs from the reference Hugging Face tokenizer.
`embeddings.json` holds, for text pairs, the integer dot product and squared norms that the
quantized model must give. Rust must reproduce both exactly on every platform.
"""

import argparse
import json
import re
import struct
from pathlib import Path

import numpy as np
from tokenizers import Tokenizer

EDGE_CASES = [
    "Hello, World!",
    "  leading and trailing   spaces  ",
    "tabs\tand\nnewlines\r\nmixed",
    "Café naïve résumé façade Zürich",
    "ÀÉÎÕÜ uppercase accents",
    "ΣΊΣΥΦΟΣ final sigma ΟΔΥΣΣΕΥΣ",
    "İstanbul dotted capital I",
    "straße ǅ ǈ titlecase digraphs",
    "中文汉字混合English text",
    "日本語のテキストとカタカナ",
    "한국어 텍스트 example",
    "emoji 🚀 and 👩‍💻 sequences ✅",
    "snake_case_identifier and camelCaseIdentifier and kebab-case-name",
    "https://example.com/path?query=1&other=2#fragment",
    "user@example.com wrote: \"quoted\" text — with dashes – and … ellipsis",
    "3.14159 and 1,000,000 and 0xDEADBEEF and 2026-10-08",
    "$100 + 50% = <html> ^caret ~tilde |pipe| `backtick`",
    "«guillemets» „low quotes“ ‹single› ¿question? ¡exclaim!",
    "zero​width‌joiners⁠here and soft­hyphen",
    "control\x00chars\x07bell\x1bescape and replacement � char",
    "nbsp space and ideographic　space and thin space",
    "a" * 150,
    "supercalifragilisticexpialidocious pneumonoultramicroscopicsilicovolcanoconiosis",
    "xyzzyplugh qwertyuiop asdfghjkl zxcvbnm",
    "",
    " ",
    "....,,,,;;;;",
    "x",
    "[CLS] [SEP] [MASK] [UNK] [PAD] literal specials",
    "##subword ##prefix handling",
    "ﬁne ﬂuent ligatures and ① circled ⑩ digits and ² superscript",
    "é combining acute and ä combining diaeresis",
    "Ünïcödé in 'single quotes' and (parentheses) and {braces} and [brackets]",
]


def corpus_sentences(paths):
    found = []
    for path in paths:
        text = Path(path).read_text(encoding="utf-8")
        for sentence in re.split(r"(?<=[.!?])\s+|\n\s*\n", text):
            sentence = " ".join(sentence.split())
            if sentence:
                found.append(sentence)
    return found


def load_matrix(path):
    data = Path(path).read_bytes()
    assert data[:4] == b"SCPG"
    _, dims, count, specials = struct.unpack("<HHIH", data[4:14])
    offset = 14 + 4 * specials
    for _ in range(count):
        (length,) = struct.unpack("<H", data[offset : offset + 2])
        offset += 2 + length
    matrix = np.frombuffer(data, dtype=np.int8, count=count * dims, offset=offset)
    return matrix.reshape(count, dims), data[14 : 14 + 4 * specials]


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--model-dir", required=True, type=Path)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--corpus", nargs="+", required=True)
    args = parser.parse_args()

    tokenizer = Tokenizer.from_file(str(args.model_dir / "tokenizer.json"))
    pool = corpus_sentences(args.corpus)
    texts = EDGE_CASES + pool[:: max(1, len(pool) // 480)]
    golden = [[text, tokenizer.encode(text, add_special_tokens=False).ids] for text in texts]
    (args.out / "tokenizer.json").write_text(json.dumps(golden, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")

    matrix, raw_specials = load_matrix(args.model)
    specials = set(struct.unpack(f"<{len(raw_specials) // 4}I", raw_specials))
    long_pool = [s for s in pool if len(s.split()) >= 8]
    pairs = []
    for index in range(0, len(long_pool) - 1, max(1, len(long_pool) // 60)):
        pairs.append((long_pool[index], long_pool[index + 1]))
    entries = []
    for first, second in pairs:
        vectors = []
        for text in (first, second):
            ids = [i for i in tokenizer.encode(text, add_special_tokens=False).ids[:512] if i not in specials]
            vectors.append(matrix[ids].astype(np.int64).sum(axis=0) if ids else np.zeros(matrix.shape[1], np.int64))
        dot = int(vectors[0] @ vectors[1])
        entries.append([first, second, str(dot), str(int(vectors[0] @ vectors[0])), str(int(vectors[1] @ vectors[1]))])
    (args.out / "embeddings.json").write_text(json.dumps(entries, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    print(f"{len(golden)} tokenizer cases, {len(entries)} embedding pairs")


if __name__ == "__main__":
    main()
