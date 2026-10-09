.PHONY: benchmark bootstrap check field field-baseline field-polygraph field-polygraph-baseline polygraph-lm polygraph-model self-lint web web-check web-model web-serve

bootstrap:
	cargo build --locked
	@if command -v pre-commit >/dev/null 2>&1; then \
		pre-commit install --hook-type pre-commit --hook-type commit-msg; \
	elif command -v uvx >/dev/null 2>&1; then \
		uvx pre-commit install --hook-type pre-commit --hook-type commit-msg; \
	else \
		echo "Install pre-commit or uv, then run make bootstrap again." >&2; \
		exit 1; \
	fi

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -W clippy::all -W clippy::pedantic -D warnings
	cargo test --all-targets --all-features
	$(MAKE) self-lint

self-lint:
	cargo run --quiet -- .

benchmark:
	cargo run --release -- benchmark

field:
	cargo build --release --locked
	python3 -I benchmarks/field/run.py check

field-baseline:
	cargo build --release --locked
	python3 -I benchmarks/field/run.py update

web:
	cd web && wasm-pack build --target web --out-dir site/pkg --no-pack --no-typescript --release
	node web/build-api.mjs

web-check:
	cargo check --manifest-path web/Cargo.toml --target wasm32-unknown-unknown --locked
	node --test web/test/*.mjs

web-serve: web
	python3 -m http.server --directory web/site 8000

POLYGRAPH_CACHE ?= $(or $(XDG_CACHE_HOME),$(HOME)/.cache)/slopcop
POLYGRAPH_MODEL_NAME := polygraph-ec9c31b3ba4a.bin
POLYGRAPH_MODEL_SHA256 := ec9c31b3ba4ad39c9856f91484b883316078b35215e3b4e0ae04e4c797880a93
POLYGRAPH_MODEL_URL ?= https://github.com/JGalego/slopcop/releases/download/polygraph-model-1/$(POLYGRAPH_MODEL_NAME)

# Like `field`, with the polygraph embedding rules. Needs the model: run `make polygraph-model`.
field-polygraph:
	cargo build --release --locked --features polygraph
	python3 -I benchmarks/field/run.py check --polygraph

field-polygraph-baseline:
	cargo build --release --locked --features polygraph
	python3 -I benchmarks/field/run.py update --polygraph

# Downloads the embedding model that the polygraph tests and rules need, and checks its SHA-256.
polygraph-model:
	SLOPCOP_CACHE="$(POLYGRAPH_CACHE)" sh polygraph/fetch.sh model

# Downloads the SmolLM2-135M files for POLY001 and POLY002 and checks their SHA-256 sums.
polygraph-lm:
	SLOPCOP_CACHE="$(POLYGRAPH_CACHE)" sh polygraph/fetch.sh lm

# Puts the model where the website serves it: the cached copy when there is one, else the release.
web-model:
	mkdir -p web/site/models
	if [ -f "$(POLYGRAPH_CACHE)/$(POLYGRAPH_MODEL_NAME)" ]; then \
		cp "$(POLYGRAPH_CACHE)/$(POLYGRAPH_MODEL_NAME)" web/site/models/; \
	else \
		curl -fL -o web/site/models/$(POLYGRAPH_MODEL_NAME) "$(POLYGRAPH_MODEL_URL)"; \
	fi
	echo "$(POLYGRAPH_MODEL_SHA256)  web/site/models/$(POLYGRAPH_MODEL_NAME)" | sha256sum -c
