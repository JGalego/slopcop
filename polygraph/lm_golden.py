#!/usr/bin/env python3
"""Write the golden per-token surprisals that pin the Rust language-model forward pass.

    lm_golden.py --lm-dir DIR --out tests/polygraph/lm.json

DIR holds `model.safetensors`, `tokenizer.json` and `config.json` from HuggingFaceTB/SmolLM2-135M
at revision 93efa2f097d58c2a74874c7e644dbc9b0cee75a2 (Apache-2.0). Each case is scored the way the
Rust code scores a window: the end-of-text token (id 0) is prepended, and the surprisal of every
following token, in bits, comes from the full-precision float32 model.

Dependencies (tested versions): torch 2.x CPU, transformers 4.x, tokenizers 0.23.2.
"""

import argparse
import json
import math
from pathlib import Path

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

TEXTS = [
    "The scanner reads every tracked file in parallel and reports findings in a stable order.",
    "In today's fast-paced world, it is important to remember that quality matters. Quality is something that matters in every project.",
    "Rust's borrow checker rejects programs that could read freed memory, which removes a whole class of bugs at compile time.",
    "The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.",
    "Set the timeout to 30 seconds, then restart the daemon with `systemctl restart ingestd`.",
    "Café naïve résumé: unusual characters, 中文汉字 and emoji 🚀 appear here too.",
    "xyzzy plugh qwerty asdf zxcv uiop hjkl. Colorless green ideas sleep furiously.",
    "A paragraph of ordinary documentation explains what the option does, when to use it, and what happens when it is left unset.\n\nA second paragraph starts after a blank line and continues with a few more sentences about defaults.",
]


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--lm-dir", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    tokenizer = AutoTokenizer.from_pretrained(args.lm_dir)
    model = AutoModelForCausalLM.from_pretrained(args.lm_dir, torch_dtype=torch.float32)
    model.eval()
    cases = []
    for text in TEXTS:
        ids = tokenizer(text, add_special_tokens=False)["input_ids"]
        inputs = torch.tensor([[0] + ids])
        with torch.no_grad():
            logits = model(inputs).logits[0, :-1].float()
        log_probs = torch.log_softmax(logits, dim=-1)
        bits = [-log_probs[i, token].item() / math.log(2) for i, token in enumerate(ids)]
        cases.append({"text": text, "ids": ids, "bits": [round(value, 5) for value in bits]})
    args.out.write_text(json.dumps(cases, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{len(cases)} cases, {sum(len(case['ids']) for case in cases)} tokens")


if __name__ == "__main__":
    main()
