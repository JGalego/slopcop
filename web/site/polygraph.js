// Fetches the Polygraph model that the WebAssembly module accepts, checks its SHA-256, and keeps it
// in the browser's cache so that it downloads once. The demo's worker and the public module at
// api/v1/slopcop.js both call it after the WebAssembly module is initialized.
import { polygraph_model as describeModel } from "./pkg/slopcop_web.js";

const CACHE = "slopcop-polygraph";

// Returns the model as a Uint8Array. `onProgress(stage)` is called before a download starts.
export async function loadModel(onProgress = () => {}) {
  const { file, sha256 } = JSON.parse(describeModel());
  const url = new URL(`./models/${file}`, import.meta.url);

  let cache = null;
  try {
    cache = await caches.open(CACHE);
  } catch {
    // The Cache API is missing in some private windows; the model then downloads on every scan.
  }
  let response = cache ? await cache.match(url).catch(() => undefined) : undefined;
  if (!response) {
    onProgress("Downloading Polygraph model");
    response = await fetch(url);
    if (!response.ok) {
      throw Object.assign(new Error(`The Polygraph model could not be downloaded (${response.status}).`), { code: "polygraph-model" });
    }
    await cache?.put(url, response.clone()).catch(() => {});
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  try {
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    const actual = Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
    if (actual !== sha256) {
      await cache?.delete(url).catch(() => {});
      throw Object.assign(new Error("The downloaded Polygraph model does not match its checksum."), { code: "polygraph-model" });
    }
  } catch (error) {
    if (error.code === "polygraph-model") throw error;
    // crypto.subtle exists only in secure contexts; the WebAssembly module checks the hash again.
  }
  return bytes;
}
