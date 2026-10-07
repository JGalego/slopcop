// Writes the static API endpoints under site/api/v1 from the WebAssembly build in site/pkg, and
// checks a sample report against site/api/v1/report.schema.json so the schema cannot drift from
// the linter. Run by `make web` after wasm-pack.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { initSync, rules, version, Scanner } from "./site/pkg/slopcop_web.js";

const site = new URL("./site/", import.meta.url);
const api = new URL("api/v1/", site);
initSync({ module: readFileSync(new URL("pkg/slopcop_web_bg.wasm", site)) });

const write = (path, value) => writeFileSync(new URL(path, api), JSON.stringify(value, null, 2) + "\n");
const metadata = JSON.parse(rules());

rmSync(new URL("rules/", api), { recursive: true, force: true });
mkdirSync(new URL("rules/", api), { recursive: true });
write("version.json", { version: version(), rule_count: metadata.length });
write("rules.json", metadata);
for (const rule of metadata) {
  write(`rules/${rule.id}.json`, rule);
}

const scanner = new Scanner(undefined);
scanner.add("app.py", new TextEncoder().encode("def load():\n    try:\n        run()\n    except Exception:\n        pass\n"));
const report = JSON.parse(scanner.finish());
scanner.free();
const schema = JSON.parse(readFileSync(new URL("report.schema.json", api)));
const problems = validate(report, schema, schema, "report");
if (report.findings.length === 0) problems.push("the sample report has no findings to check");
if (problems.length > 0) {
  console.error(`report.schema.json does not match the linter's report:\n  ${problems.join("\n  ")}`);
  process.exit(1);
}
console.log(`Wrote the API for slopcop ${version()} with ${metadata.length} rules.`);

// Checks the subset of JSON Schema that report.schema.json uses.
function validate(value, node, root, at) {
  if (node.$ref) return validate(value, node.$ref.slice(2).split("/").reduce((part, key) => part[key], root), root, at);
  const problems = [];
  const type = value === null ? "null" : Array.isArray(value) ? "array" : Number.isInteger(value) ? "integer" : typeof value;
  const types = [node.type ?? []].flat();
  if (types.length > 0 && !types.includes(type)) return [`${at} is ${type}, expected ${types.join(" or ")}`];
  if (node.enum && !node.enum.includes(value)) problems.push(`${at} is ${JSON.stringify(value)}, expected one of ${node.enum.join(", ")}`);
  if (node.pattern && !new RegExp(node.pattern).test(value)) problems.push(`${at} does not match ${node.pattern}`);
  if (node.minimum !== undefined && value < node.minimum) problems.push(`${at} is below ${node.minimum}`);
  if (type === "array" && node.items) value.forEach((item, index) => problems.push(...validate(item, node.items, root, `${at}[${index}]`)));
  if (type === "object") {
    for (const key of node.required ?? []) if (!(key in value)) problems.push(`${at}.${key} is missing`);
    for (const [key, item] of Object.entries(value)) {
      if (node.properties?.[key]) problems.push(...validate(item, node.properties[key], root, `${at}.${key}`));
      else if (node.additionalProperties === false) problems.push(`${at}.${key} is not in the schema`);
    }
  }
  return problems;
}
