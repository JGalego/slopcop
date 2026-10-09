import assert from "node:assert/strict";
import { test } from "node:test";

import { loadVerifiedFile } from "../site/model-cache.js";

const sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

test("a checksum mismatch evicts the cached copy", async (t) => {
  t.after(() => {
    delete globalThis.fetch;
    delete globalThis.caches;
  });

  const deleted = [];
  const stored = new Map();
  globalThis.caches = {
    open: async () => ({
      match: async (url) => stored.get(String(url)),
      put: async (url, response) => {
        stored.set(String(url), response);
      },
      delete: async (url) => {
        deleted.push(String(url));
        stored.delete(String(url));
      },
    }),
  };
  globalThis.fetch = async () =>
    new Response("not the empty file", { status: 200, headers: { "content-length": "16" } });

  await assert.rejects(
    () =>
      loadVerifiedFile(
        { url: "https://example.test/empty.bin", sha256 },
        { label: "Polygraph embedding model", code: "polygraph-model" },
      ),
    (error) => error.code === "polygraph-model" && /checksum/.test(error.message),
  );
  assert.equal(deleted.length, 1);
});

test("a missing file keeps a stable error code", async (t) => {
  t.after(() => {
    delete globalThis.fetch;
    delete globalThis.caches;
  });

  globalThis.caches = { open: async () => ({ match: async () => undefined }) };
  globalThis.fetch = async () => new Response("missing", { status: 404 });

  await assert.rejects(
    () =>
      loadVerifiedFile(
        { url: "https://example.test/missing.bin", sha256 },
        { label: "Polygraph language model", code: "polygraph-lm" },
      ),
    (error) => error.code === "polygraph-lm" && /404/.test(error.message),
  );
});
