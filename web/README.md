# Browser demo

This directory holds the GitHub Pages site that scans a public repository in the browser. The `slopcop-web` crate wraps the library with `wasm-bindgen`; `site/` is a static page with no build step beyond the WebAssembly package.

## Running locally

Install the `wasm32-unknown-unknown` target and `wasm-pack`, then from the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
make web-serve
```

Open <http://localhost:8000>. Append `#owner/repo` or `#owner/repo@ref` to the URL to start a scan directly. After changing Rust code, run `make web` again; page changes only need a reload.

## How a scan works

1. `worker.js` resolves the ref to a commit with the GitHub REST API and lists the tree in one recursive request. Anonymous clients get 60 API requests per hour, and each scan uses two.
2. The scanner decides which blobs to download using the same rules as the CLI: known source types only, no dependency or build directories, the configured size limit, and the repository's `.slopcop.toml` ignore and include globs.
3. Selected files are downloaded from `raw.githubusercontent.com`, pinned to the commit, with bounded concurrency. The demo caps a scan at 3,000 files or 60 MB.
4. The worker passes the bytes to `Scanner::add`, and `Scanner::finish` returns the same JSON report as `slopcop --format json`.

Two behaviors differ from a local run. Files with unrecognized extensions are never downloaded, although the CLI reads them, and `papertrail` history checks need a clone, so they do not run here. Private repositories are not supported.
