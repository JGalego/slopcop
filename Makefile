.PHONY: benchmark bootstrap check self-lint web web-serve

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

web:
	cd web && wasm-pack build --target web --out-dir site/pkg --no-pack --no-typescript --release
	node web/build-api.mjs

web-serve: web
	python3 -m http.server --directory web/site 8000
