// Shared Polygraph option parsing for the demo worker and the public API.
// `true` keeps the historical meaning: embeddings only.

export function polygraphModes(value) {
  if (value && typeof value === "object") {
    return {
      embeddings: Boolean(value.embeddings),
      languageModel: Boolean(value.languageModel),
    };
  }
  return { embeddings: Boolean(value), languageModel: false };
}

export function polygraphCacheVariant(modes) {
  return `${modes.embeddings ? "e" : "-"}${modes.languageModel ? "l" : "-"}`;
}
