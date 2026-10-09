// The slopcop JavaScript API. It runs the same WebAssembly build as https://slopcop.me/ in the
// caller's browser or Deno process; nothing is sent to slopcop.me. See https://slopcop.me/api/.
//
//   import { scanFiles, scanRepository } from "https://slopcop.me/api/v1/slopcop.js";
import init, { Scanner, rules as ruleList, version as linterVersion } from "../../pkg/slopcop_web.js";
import { parseRepository } from "../../forges.js";
import { polygraphModes } from "../../polygraph-options.js";
import { loadLanguageModel, loadModel } from "../../polygraph.js";
import { CONFIG_FILE, resolve, scanRepository as scanCommit } from "../../scan.js";

let ready;
const load = () => (ready ??= init({ module_or_path: new URL("../../pkg/slopcop_web_bg.wasm", import.meta.url) }));

const failure = (message, code) => Object.assign(new Error(message), { code });

// Returns the slopcop version this module runs, such as "0.4.0".
export async function version() {
  await load();
  return linterVersion();
}

// Returns the metadata of every rule, as served by /api/v1/rules.json.
export async function rules() {
  await load();
  return JSON.parse(ruleList());
}

// Scans in-memory files and returns the report that `slopcop --format json` prints.
//
// `files` maps repository-relative paths to contents, as a plain object, a Map, or any iterable of
// [path, content] pairs. Contents are strings, Uint8Arrays, or ArrayBuffers. A `.slopcop.toml` or
// `.gitattributes` among the files applies as it would in a checkout, and `options.config` takes
// the place of `.slopcop.toml`. Files the CLI would skip count as skipped files. Pass
// `polygraph: true` runs POLY003/004 and downloads a 4 MB model once. Pass
// `polygraph: { embeddings: true, languageModel: true }` to include POLY001/002; their pinned
// language model downloads about 259 MB and findings may vary between devices.
export async function scanFiles(files, { config, polygraph = false } = {}) {
  await load();
  const entries = (Symbol.iterator in Object(files) ? [...files] : Object.entries(files ?? {}))
    .map(([path, content]) => [String(path).replace(/^\.?\//, ""), bytesOf(path, content)]);
  const decoder = new TextDecoder();
  config ??= entries.find(([path]) => path === CONFIG_FILE)?.[1];

  let scanner;
  try {
    scanner = new Scanner(config instanceof Uint8Array ? decoder.decode(config) : config);
  } catch (error) {
    throw failure(`Invalid ${CONFIG_FILE}: ${error.message}`, "bad-config");
  }
  try {
    const models = await loadPolygraph(polygraph);
    if (models.embedding) scanner.load_polygraph(models.embedding);
    if (models.language) scanner.load_polygraph_lm(models.language.weights, models.language.tokenizer);
    for (const [path, bytes] of entries) {
      if (path.split("/").pop() === ".gitattributes") scanner.attributes(path, decoder.decode(bytes));
    }
    let skipped = 0;
    for (const [path, bytes] of entries) {
      if (scanner.wants(path, bytes.length)) scanner.add(path, bytes);
      else skipped += 1;
    }
    const report = JSON.parse(scanner.finish());
    report.summary.skipped_files += skipped;
    return report;
  } finally {
    scanner.free();
  }
}

// Scans a public GitHub, GitLab, or Codeberg repository at a branch, tag, or commit.
//
// `repo` is `owner/name`, `gitlab.com/group/project`, `codeberg.org/owner/name`, or a web URL of
// any of them. Options:
//   ref         branch, tag, or commit; overrides a ref in the URL. Defaults to the default branch.
//   token       a GitHub token, to raise GitHub's anonymous limit of 60 API requests per hour.
//               It is only sent to api.github.com.
//   onProgress  called as onProgress({ stage, done, total }) while the scan runs.
//   polygraph   true runs POLY003/004. Use { embeddings, languageModel } to select model tiers.
export async function scanRepository(repo, { ref, token, onProgress, polygraph = false } = {}) {
  await load();
  const target = typeof repo === "string" ? parseRepository(repo) : null;
  if (!target) {
    throw failure("Name a repository as owner/name, or a GitHub, GitLab, or Codeberg URL.", "bad-repository");
  }
  const report = (stage, done = 0, total = 0) => onProgress?.({ stage, done, total });
  ref ||= target.ref;
  report("Resolving " + (ref || "default branch"));
  const sha = await resolve(target.repo, ref, token);
  const models = await loadPolygraph(polygraph, report);
  const scanned = await scanCommit(target.repo, ref, sha, token, report, {
    polygraph: models.embedding,
    polygraphLm: models.language,
  });
  return {
    repo: scanned.repo,
    ref: scanned.ref,
    sha: scanned.sha,
    report: scanned.result,
    html: scanned.html,
    notices: scanned.notices,
    stats: scanned.stats,
  };
}

async function loadPolygraph(value, onProgress = () => {}) {
  const modes = polygraphModes(value);
  return {
    embedding: modes.embeddings ? await loadModel(onProgress) : undefined,
    language: modes.languageModel ? await loadLanguageModel(onProgress) : undefined,
  };
}

function bytesOf(path, content) {
  if (typeof content === "string") return new TextEncoder().encode(content);
  if (content instanceof Uint8Array) return content;
  if (content instanceof ArrayBuffer) return new Uint8Array(content);
  throw failure(`The contents of ${path} must be a string, Uint8Array, or ArrayBuffer.`, "bad-file");
}
