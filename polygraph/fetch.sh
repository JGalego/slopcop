#!/bin/sh
# Downloads and verifies the files that the polygraph rules and tests need.
#
#   fetch.sh model   the embedding model (POLY003, POLY004)
#   fetch.sh lm      SmolLM2-135M (POLY001, POLY002)
#
# Files go to $SLOPCOP_CACHE, else $XDG_CACHE_HOME/slopcop, else ~/.cache/slopcop. When
# $GITHUB_ENV is set, the matching environment variable is exported for later workflow steps.
set -eu

cache="${SLOPCOP_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/slopcop}"
model_name="polygraph-ec9c31b3ba4a.bin"
model_sha="ec9c31b3ba4ad39c9856f91484b883316078b35215e3b4e0ae04e4c797880a93"
model_url="${POLYGRAPH_MODEL_URL:-https://github.com/JGalego/slopcop/releases/download/polygraph-model-1/$model_name}"
lm_url="https://huggingface.co/HuggingFaceTB/SmolLM2-135M/resolve/93efa2f097d58c2a74874c7e644dbc9b0cee75a2"
weights_sha="80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1"
tokenizer_sha="9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c"

verify() {
    if command -v sha256sum >/dev/null 2>&1; then
        echo "$1  $2" | sha256sum -c -
    else
        echo "$1  $2" | shasum -a 256 -c -
    fi
}

# Downloads $1 to $2 unless $2 already has the SHA-256 $3.
fetch() {
    if [ -f "$2" ] && verify "$3" "$2" >/dev/null 2>&1; then
        echo "$2 is up to date"
        return
    fi
    curl -fL --retry 3 -o "$2" "$1"
    verify "$3" "$2"
}

export_variable() {
    value=$2
    # Git Bash expands $HOME to /c/..., which native Windows programs interpret
    # relative to the current drive. Export a native absolute path instead.
    if command -v cygpath >/dev/null 2>&1; then
        value=$(cygpath -w "$value")
    fi
    if [ -n "${GITHUB_ENV:-}" ]; then
        echo "$1=$value" >> "$GITHUB_ENV"
    fi
    echo "export $1=$value"
}

case "${1:-}" in
    model)
        mkdir -p "$cache"
        fetch "$model_url" "$cache/$model_name" "$model_sha"
        export_variable SLOPCOP_POLYGRAPH_MODEL "$cache/$model_name"
        ;;
    lm)
        mkdir -p "$cache/smollm2-135m"
        fetch "$lm_url/model.safetensors" "$cache/smollm2-135m/model.safetensors" "$weights_sha"
        fetch "$lm_url/tokenizer.json" "$cache/smollm2-135m/tokenizer.json" "$tokenizer_sha"
        export_variable SLOPCOP_POLYGRAPH_LM "$cache/smollm2-135m"
        ;;
    *)
        echo "usage: fetch.sh model|lm" >&2
        exit 2
        ;;
esac
