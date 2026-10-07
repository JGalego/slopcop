import init, { Scanner, rules } from "./pkg/slopcop_web.js";

const API = "https://api.github.com";
const RAW = "https://raw.githubusercontent.com";
const CONFIG_FILE = ".slopcop.toml";
const CONCURRENCY = 24;
const MAX_FILES = 3000;
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

async function resolve(repo, ref, token) {
  return (await github(`/repos/${repo}/commits/${encodeURIComponent(ref || "HEAD")}`, token, "application/vnd.github.sha")).trim();
}

async function scanRepository(repo, ref, sha, token) {
  const started = performance.now();
  const notices = [];

  report("Listing files");
  const tree = await github(`/repos/${repo}/git/trees/${sha}?recursive=1`, token);
  if (tree.truncated) {
    notices.push("GitHub truncated the file listing for this repository, so some files were not scanned.");
  }
  const blobs = tree.tree.filter((entry) => entry.type === "blob");

  let scanner = new Scanner(undefined);
  if (blobs.some((blob) => blob.path === CONFIG_FILE)) {
    const config = new TextDecoder().decode(await raw(repo, sha, CONFIG_FILE));
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
    await pool(attributes, CONCURRENCY, async (blob) => {
      try {
        scanner.attributes(blob.path, decoder.decode(await raw(repo, sha, blob.path)));
      } catch {
        notices.push(`Could not read ${blob.path}, so the paths it marks vendored or generated were scanned.`);
      }
    });
  }

  const { selected, overflow } = select(blobs.filter((blob) => scanner.wants(blob.path, blob.size)));
  if (overflow > 0) {
    notices.push(`This demo scans at most ${MAX_FILES} files and ${MAX_BYTES / 1024 / 1024} MB; ${overflow} more matching files were left out. Run the CLI for a full scan.`);
  }

  const contents = new Map();
  const failed = [];
  let done = 0;
  report("Downloading files", 0, selected.length);
  await pool(selected, CONCURRENCY, async (blob) => {
    try {
      contents.set(blob.path, await raw(repo, sha, blob.path));
    } catch {
      failed.push(blob.path);
    }
    done += 1;
    report("Downloading files", done, selected.length);
  });
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
  const html = scanner.html(title, `https://github.com/${repo}/blob/${sha}/`, notices);
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

function select(candidates) {
  const selected = [];
  let bytes = 0;
  for (const blob of candidates) {
    if (selected.length >= MAX_FILES || bytes + blob.size > MAX_BYTES) {
      break;
    }
    selected.push(blob);
    bytes += blob.size;
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

async function github(path, token, accept = "application/vnd.github+json") {
  const headers = { Accept: accept };
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }
  const response = await fetch(API + path, { headers });
  if (response.ok) {
    return accept.endsWith("sha") ? response.text() : response.json();
  }
  if (response.status === 401 && token) {
    throw Object.assign(new Error("GitHub rejected the token. It may have expired or been revoked; replace it or select Forget to scan anonymously."), { code: "bad-token" });
  }
  if ((response.status === 403 || response.status === 429) && response.headers.get("x-ratelimit-remaining") === "0") {
    const reset = new Date(Number(response.headers.get("x-ratelimit-reset")) * 1000).toLocaleTimeString();
    const message = token
      ? `Your token's GitHub API limit is used up. It resets at ${reset}.`
      : `GitHub's anonymous API limit (60 requests per hour) is used up for your network. It resets at ${reset}. Add a GitHub token to raise the limit to 5,000 requests per hour.`;
    throw Object.assign(new Error(message), { code: "rate-limit" });
  }
  if (response.status === 404 || response.status === 422) {
    throw Object.assign(new Error("Repository or ref not found. Only public repositories can be scanned."), { code: "not-found" });
  }
  throw new Error(`GitHub responded with ${response.status} ${response.statusText}.`);
}

async function raw(repo, sha, path, attempts = 2) {
  const url = `${RAW}/${repo}/${sha}/${path.split("/").map(encodeURIComponent).join("/")}`;
  const response = await fetch(url);
  if (response.ok) {
    return new Uint8Array(await response.arrayBuffer());
  }
  if (attempts > 1 && response.status >= 500) {
    return raw(repo, sha, path, attempts - 1);
  }
  throw new Error(`${response.status} for ${path}`);
}
