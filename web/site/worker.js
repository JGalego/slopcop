import init, { Scanner, rules } from "./pkg/slopcop_web.js";

const API = "https://api.github.com";
const RAW = "https://raw.githubusercontent.com";
const CONFIG_FILE = ".slopcop.toml";
const CONCURRENCY = 24;
const MAX_FILES = 3000;
const MAX_BYTES = 60 * 1024 * 1024;

const ready = init();

self.onmessage = async ({ data }) => {
  try {
    await ready;
    self.postMessage({ type: "done", ...(await scanRepository(data.repo, data.ref, data.token)) });
  } catch (error) {
    self.postMessage({ type: "error", message: error.message, code: error.code });
  }
};

function report(stage, done = 0, total = 0) {
  self.postMessage({ type: "progress", stage, done, total });
}

async function scanRepository(repo, ref, token) {
  const started = performance.now();
  const notices = [];

  report("Resolving " + (ref || "default branch"));
  const sha = (await github(`/repos/${repo}/commits/${encodeURIComponent(ref || "HEAD")}`, token, "application/vnd.github.sha")).trim();

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
    rules: JSON.parse(rules()),
    sources,
    notices,
    html,
    stats: {
      treeFiles: blobs.length,
      downloaded: contents.size,
      totalMillis: performance.now() - started,
      scanMillis,
    },
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
