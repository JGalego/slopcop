// Fetches the Polygraph models that the WebAssembly module accepts. SHA-256 is checked here and
// again in WebAssembly; valid files stay in the browser cache.
import {
  polygraph_language_model as describeLanguageModel,
  polygraph_model as describeModel,
} from "./pkg/slopcop_web.js";
import { loadVerifiedFile } from "./model-cache.js";

export async function loadModel(onProgress = () => {}) {
  const { file, sha256 } = JSON.parse(describeModel());
  return loadVerifiedFile(
    { url: `./models/${file}`, sha256, base: import.meta.url },
    { label: "Polygraph embedding model", code: "polygraph-model", onProgress },
  );
}

export async function loadLanguageModel(onProgress = () => {}) {
  const { weights, tokenizer } = JSON.parse(describeLanguageModel());
  return {
    tokenizer: await loadVerifiedFile(tokenizer, {
      label: "Polygraph tokenizer",
      code: "polygraph-lm",
      onProgress,
    }),
    weights: await loadVerifiedFile(weights, {
      label: "Polygraph language model",
      code: "polygraph-lm",
      onProgress,
    }),
  };
}
