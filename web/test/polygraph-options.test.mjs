import assert from "node:assert/strict";
import { test } from "node:test";

import { polygraphCacheVariant, polygraphModes } from "../site/polygraph-options.js";

test("true keeps the historical embeddings-only API", () => {
  assert.deepEqual(polygraphModes(true), { embeddings: true, languageModel: false });
  assert.equal(polygraphCacheVariant(polygraphModes(true)), "e-");
});

test("false and missing values download nothing", () => {
  assert.deepEqual(polygraphModes(false), { embeddings: false, languageModel: false });
  assert.deepEqual(polygraphModes(undefined), { embeddings: false, languageModel: false });
  assert.equal(polygraphCacheVariant(polygraphModes(false)), "--");
});

test("object form selects each model tier independently", () => {
  assert.deepEqual(polygraphModes({ languageModel: true }), {
    embeddings: false,
    languageModel: true,
  });
  assert.deepEqual(polygraphModes({ embeddings: true, languageModel: true }), {
    embeddings: true,
    languageModel: true,
  });
  assert.equal(polygraphCacheVariant(polygraphModes({ languageModel: true })), "-l");
  assert.equal(polygraphCacheVariant(polygraphModes({ embeddings: true, languageModel: true })), "el");
});
