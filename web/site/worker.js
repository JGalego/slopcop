import init, { rules } from "./pkg/slopcop_web.js";
import { polygraphCacheVariant, polygraphModes } from "./polygraph-options.js";
import { resolve, scanRepository } from "./scan.js";
import { loadLanguageModel, loadModel } from "./polygraph.js";

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

async function scan({ repo, ref, token, fresh, polygraph }, build) {
  const modes = polygraphModes(polygraph);
  // Reports from each model tier differ, so both choices are part of the key.
  const variant = polygraphCacheVariant(modes);
  const key = build && `${build}:polygraph-${variant}:${repo.toLowerCase()}@${ref}`;
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

  const model = modes.embeddings ? await loadModel(report) : undefined;
  const languageModel = modes.languageModel ? await loadLanguageModel(report) : undefined;
  const scanned = await scanRepository(repo, ref, sha, token, report, {
    polygraph: model,
    polygraphLm: languageModel,
  });
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
