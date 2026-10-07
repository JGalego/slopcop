// Scans one commit of a public repository with the WebAssembly linter. The demo's worker and the
// public module at api/v1/slopcop.js both call it once the WebAssembly module is initialized.
import { Scanner } from "./pkg/slopcop_web.js";
import { forgeOf, onThrottle } from "./forges.js";

export const CONFIG_FILE = ".slopcop.toml";
export const MAX_BYTES = 60 * 1024 * 1024;

// Resolves a branch, tag, or commit to a full commit SHA. The token is a GitHub token, so it is
// never sent to another forge.
export function resolve(repo, ref, token) {
  const { forge, path } = forgeOf(repo);
  return forge.resolve(path, ref, token);
}

// Reports progress as `onProgress(stage, done, total)`.
export async function scanRepository(repo, ref, sha, token, onProgress = () => {}) {
  const { forge } = forgeOf(repo);
  const stopWatching = onThrottle((throttled, millis) => {
    if (throttled === forge) onProgress(`${forge.name} is rate-limiting this scan; waiting ${Math.ceil(millis / 1000)} s`);
  });
  try {
    return await scanCommit(repo, ref, sha, token, onProgress);
  } finally {
    stopWatching();
  }
}

async function scanCommit(repo, ref, sha, token, report) {
  const started = performance.now();
  const notices = [];
  const { forge, path } = forgeOf(repo);
  const raw = (file) => forge.raw(path, sha, file);

  report("Listing files");
  const { blobs, truncated } = await forge.list(path, sha, token);
  if (truncated) {
    notices.push(`${forge.name} truncated the file listing for this repository, so some files were not scanned.`);
  }

  let scanner = new Scanner(undefined);
  if (blobs.some((blob) => blob.path === CONFIG_FILE)) {
    const config = new TextDecoder().decode(await raw(CONFIG_FILE));
    try {
      scanner = new Scanner(config);
      notices.push(`Applied the repository's ${CONFIG_FILE}.`);
    } catch (error) {
      notices.push(`Ignored the repository's ${CONFIG_FILE}: ${error.message}`);
    }
  }

  const attributes = blobs.filter((blob) => blob.path.split("/").pop() === ".gitattributes");
  if (attributes.length > 0) {
    report("Reading .gitattributes");
    const decoder = new TextDecoder();
    await pool(attributes, forge.concurrency, async (blob) => {
      try {
        scanner.attributes(blob.path, decoder.decode(await raw(blob.path)));
      } catch {
        notices.push(`Could not read ${blob.path}, so the paths it marks vendored or generated were scanned.`);
      }
    });
  }

  // GitLab lists no blob sizes, so its files are checked against the size limits after download.
  const { selected, overflow } = select(blobs.filter((blob) => scanner.wants(blob.path, blob.size ?? 0)), forge.maxFiles);
  let left = overflow;

  const contents = new Map();
  const failed = [];
  let done = 0;
  let bytes = 0;
  report("Downloading files", 0, selected.length);
  await pool(selected, forge.concurrency, async (blob) => {
    try {
      if (bytes < MAX_BYTES) {
        const data = await raw(blob.path);
        if (scanner.wants(blob.path, data.length)) {
          contents.set(blob.path, data);
          bytes += data.length;
        }
      } else {
        left += 1;
      }
    } catch {
      failed.push(blob.path);
    }
    done += 1;
    report("Downloading files", done, selected.length);
  });
  if (left > 0) {
    notices.push(`Browser scans read at most ${forge.maxFiles} files and ${MAX_BYTES / 1024 / 1024} MB from ${forge.name}; ${left} more matching files were left out. Run the CLI for a full scan.`);
  }
  if (failed.length > 0) {
    notices.push(`${failed.length} file(s) could not be downloaded and were not scanned: ${failed.slice(0, 5).join(", ")}${failed.length > 5 ? ", …" : ""}`);
  }

  report("Scanning", selected.length, selected.length);
  const scanStarted = performance.now();
  for (const [path, bytes] of contents) {
    scanner.add(path, bytes);
  }
  const result = JSON.parse(scanner.finish());
  const scanMillis = performance.now() - scanStarted;
  const title = `${repo} · ${ref ? ref + " · " : ""}${sha.slice(0, 7)}`;
  const html = scanner.html(title, forge.blobBase(path, sha), notices);
  scanner.free();

  const decoder = new TextDecoder();
  const sources = {};
  for (const finding of result.findings) {
    if (!(finding.path in sources) && contents.has(finding.path)) {
      sources[finding.path] = decoder.decode(contents.get(finding.path));
    }
  }

  return {
    repo,
    ref: ref || null,
    sha,
    result,
    sources,
    notices,
    html,
    stats: {
      treeFiles: blobs.length,
      downloaded: contents.size,
      failed: failed.length,
      totalMillis: performance.now() - started,
      scanMillis,
    },
  };
}

function select(candidates, maxFiles) {
  const selected = [];
  let bytes = 0;
  for (const blob of candidates) {
    if (selected.length >= maxFiles || bytes + (blob.size ?? 0) > MAX_BYTES) {
      break;
    }
    selected.push(blob);
    bytes += blob.size ?? 0;
  }
  return { selected, overflow: candidates.length - selected.length };
}

async function pool(items, limit, task) {
  let next = 0;
  const lanes = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) {
      await task(items[next++]);
    }
  });
  await Promise.all(lanes);
}
