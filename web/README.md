# Browser demo

This directory holds the GitHub Pages site that scans a public repository in the browser. The `slopcop-web` crate wraps the library with `wasm-bindgen`; `site/` is a static page with no build step beyond the WebAssembly package.

## Running locally

Install the `wasm32-unknown-unknown` target and `wasm-pack`, then from the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
make web-serve
```

Open <http://localhost:8000>. Append `#owner/repo` or `#owner/repo@ref` to the URL to start a scan directly. GitHub repositories are named `owner/repo`; GitLab and Codeberg repositories carry their host, as in `#gitlab.com/group/project` or `#codeberg.org/owner/repo@ref`. After changing Rust code, run `make web` again; page changes only need a reload.

## How a scan works

1. `worker.js` resolves the ref to a commit and lists the commit's files through the forge's API. `forges.js` holds what differs between GitHub, GitLab, and Codeberg: the API calls, the file downloads, the rate limits, and the links back to each file.
   - **GitHub:** the tree comes in one recursive request. Anonymous clients get 60 API requests per hour per IP address, and each scan uses two. The optional token field raises that to 5,000 per hour: the token is kept in `sessionStorage` for the tab and sent only with these two API requests, never to `raw.githubusercontent.com` or another forge.
   - **GitLab:** the tree comes 100 entries per page, up to 50 pages, without blob sizes. Anonymous clients get 500 API requests per minute.
   - **Codeberg:** the tree comes 1,000 entries per page, up to 20 pages. Anonymous clients get 2,000 API requests per 10 minutes. Its API does not answer CORS preflight requests, so the worker sends plain requests with no headers.
2. The scanner decides which blobs to download using the same rules as the CLI: known source types only, no dependency or build directories, the configured size limit, and the repository's `.slopcop.toml` ignore and include globs.
3. Selected files are downloaded at the commit with bounded concurrency. GitHub serves them from `raw.githubusercontent.com`, which does not count against the API limit, so a scan takes up to 3,000 files. GitLab and Codeberg serve them through the API, one request per file, so a scan takes up to 400 files from GitLab, which keeps one scan under its limit, and up to 1,000 from Codeberg. Every scan stops at 60 MB. GitLab lists no sizes, so its files are checked against the size limit after download. Neither GitLab nor Codeberg lets the page read its rate-limit headers, so a throttled request waits a minute and is retried up to three times, and the progress line says so.
4. The worker passes the bytes to `Scanner::add`, and `Scanner::finish` returns the same JSON report as `slopcop --format json`. `Scanner::html` then renders the same page as `slopcop --format html`, with the scan notices and links to the commit on its forge, for the Download HTML button.

## Cached reports

The worker keeps the 10 most recent complete reports in IndexedDB, keyed by repository and ref. A key also carries a hash of the WebAssembly module, so a deploy that changes any rule invalidates every cached report.

Opening or reloading a repository link shows the cached report at once and labels it with its age. The worker then makes one API request to resolve the ref, and if the branch has moved since the scan, a notice offers a rescan. A ref given as a full commit hash cannot move, so it is never checked. The Scan button always resolves the ref, and downloads and rescans files only when the commit has changed. A scan where any download failed is not cached, so the next visit tries again. When IndexedDB is blocked or full, scans run as before without a cache.

Each finding links to the false-positive issue form, prefilled with the rule, version, a link to the line, the surrounding source, and the finding output. The page sends nothing itself: GitHub shows the form, and the user reviews and submits it.

Two behaviors differ from a local run. Files with unrecognized extensions are never downloaded, although the CLI reads them, and `papertrail` history checks need a clone, so they do not run here. Private repositories are not supported.
