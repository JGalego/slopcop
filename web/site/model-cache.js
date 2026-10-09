// Fetches a file, checks its SHA-256, and keeps a valid copy in the Cache API.

const CACHE = "slopcop-polygraph";

export async function loadVerifiedFile(description, { label, code, onProgress = () => {} } = {}) {
  const url = new URL(description.url, description.base);
  let cache = null;
  try {
    cache = await caches.open(CACHE);
  } catch {
    // The Cache API is missing in some private windows; the file then downloads on every scan.
  }
  let response = cache ? await cache.match(url).catch(() => undefined) : undefined;
  let cacheWrite;
  if (!response) {
    onProgress(`Downloading ${label}`, 0, 0);
    response = await fetch(url, { credentials: "omit" });
    if (!response.ok) {
      throw modelError(`${label} could not be downloaded (${response.status}).`, code);
    }
    cacheWrite = cache?.put(url, response.clone()).catch(() => {});
  }
  const bytes = await readBytes(response, `Downloading ${label}`, onProgress);
  await cacheWrite;
  try {
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    const actual = Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
    if (actual !== description.sha256) {
      await cache?.delete(url).catch(() => {});
      throw modelError(`${label} does not match its checksum.`, code);
    }
  } catch (error) {
    if (error.code === code) throw error;
    // crypto.subtle exists only in secure contexts; the WebAssembly module checks the hash again.
  }
  return bytes;
}

async function readBytes(response, stage, onProgress) {
  const total = Number(response.headers.get("content-length")) || 0;
  if (!response.body) return new Uint8Array(await response.arrayBuffer());
  const reader = response.body.getReader();
  const chunks = [];
  let done = 0;
  while (true) {
    const part = await reader.read();
    if (part.done) break;
    chunks.push(part.value);
    done += part.value.length;
    onProgress(stage, done, total);
  }
  const bytes = new Uint8Array(done);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

function modelError(message, code) {
  return Object.assign(new Error(message), { code });
}
