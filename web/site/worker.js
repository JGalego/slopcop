import init, { Scanner, rules } from "./pkg/slopcop_web.js";
import { forgeOf, onThrottle } from "./forges.js";

const CONFIG_FILE = ".slopcop.toml";
const MAX_BYTES = 60 * 1024 * 1024;
const DATABASE = "slopcop";
const REPORTS = "reports";
const KEEP_REPORTS = 10;
const FULL_SHA = /^[0-9a-f]{40}$/i;

// Reports are cached per build, so a deploy that changes any rule invalidates them. The build is
// identified by a hash of the WebAssembly module, which is fetched once and used for both.
const ready = (async () => {
  const bytes = await (await fetch(new URL("./pkg/slopcop_web_bg.wasm", import.meta.url))).arrayBuffer();
  await init({ module_or_path: bytes });
  try {
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    return Array.from(digest.slice(0, 8), (byte) => byte.toString(16).padStart(2, "0")).join("");
  } catch {
    // crypto.subtle exists only in secure contexts. Without a build hash, reports are not cached.
    return null;
  }
})();

onThrottle((forge, millis) => {
  report(`${forge.name} is rate-limiting this scan; waiting ${Math.ceil(millis / 1000)} s`);
});

self.onmessage = async ({ data }) => {
  try {
    const build = await ready;
    if (data.type === "rules") {
      self.postMessage({ type: "rules", rules: JSON.parse(rules()) });
      return;
    }
    await scan(data, build);
  } catch (error) {
    self.postMessage({ type: "error", message: error.message, code: error.code });
  }
};

async function scan({ repo, ref, token, fresh }, build) {
  const key = build && `${build}:${repo.toLowerCase()}@${ref}`;
  const entry = key && (await cache("readonly", (store) => store.get(key)));

  // Reopening a scan shows the cached report at once, then checks whether the ref has moved.
  if (entry && !fresh) {
    sendCached(entry);
    if (!FULL_SHA.test(ref)) {
      const sha = await resolve(repo, ref, token).catch(() => null);
      if (sha && sha !== entry.sha) self.postMessage({ type: "stale", repo, ref: ref || null, sha });
    }
    return;
  }

  report("Resolving " + (ref || "default branch"));
  const sha = await resolve(repo, ref, token);
  if (entry?.sha === sha) {
    sendCached(entry);
    return;
  }

  const scanned = await scanRepository(repo, ref, sha, token);
  self.postMessage({ type: "done", ...scanned, rules: JSON.parse(rules()) });
  // A scan with failed downloads is incomplete, so it is not cached and the next visit retries.
  if (key && scanned.stats.failed === 0) {
    await cache("readwrite", (store) => {
      store.put({ sha, savedAt: Date.now(), scanned }, key);
      prune(store, build);
    });
  }
}

function sendCached(entry) {
  self.postMessage({ type: "done", ...entry.scanned, rules: JSON.parse(rules()), cachedAt: entry.savedAt });
}

function report(stage, done = 0, total = 0) {
  self.postMessage({ type: "progress", stage, done, total });
}

// The token is a GitHub token, so it is never sent to another forge.
function resolve(repo, ref, token) {
  const { forge, path } = forgeOf(repo);
  return forge.resolve(path, ref, token);
}

async function scanRepository(repo, ref, sha, token) {
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
    notices.push(`This demo scans at most ${forge.maxFiles} files and ${MAX_BYTES / 1024 / 1024} MB from ${forge.name}; ${left} more matching files were left out. Run the CLI for a full scan.`);
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

// Runs one transaction on the report store. Storage can be unavailable or full; every failure
// reads as a cache miss, and the scan goes ahead without it.
let database;
async function cache(mode, work) {
  database ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(REPORTS).createIndex("savedAt", "savedAt");
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  }).catch(() => null);
  const db = await database;
  if (!db) return undefined;
  try {
    return await new Promise((resolve, reject) => {
      const transaction = db.transaction(REPORTS, mode);
      const request = work(transaction.objectStore(REPORTS));
      transaction.oncomplete = () => resolve(request?.result);
      transaction.onerror = transaction.onabort = () => reject(transaction.error);
    });
  } catch {
    return undefined;
  }
}

// Keeps the newest reports from the current build and deletes the rest.
function prune(store, build) {
  let kept = 0;
  store.index("savedAt").openKeyCursor(null, "prev").onsuccess = ({ target }) => {
    const cursor = target.result;
    if (!cursor) return;
    if (!cursor.primaryKey.startsWith(build + ":") || ++kept > KEEP_REPORTS) store.delete(cursor.primaryKey);
    cursor.continue();
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
