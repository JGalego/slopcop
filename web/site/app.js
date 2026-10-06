const SEVERITIES = ["error", "warning", "info"];
const MODULES = ["deadweight", "vibecheck", "papertrail"];
const PAGE_SIZE = 250;
const CONTEXT_LINES = 2;
const TOKEN_KEY = "slopcop.github-token";
const REPO_URL = "https://github.com/JGalego/slopcop";
const MAX_FIELD = 1500;
const RULES_DOC = `${REPO_URL}/blob/main/docs/rules/README.md`;
const SAMPLE_RULES = 6;

const $ = (id) => document.getElementById(id);
const worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });

const state = {
  data: null,
  rules: new Map(),
  severity: new Set(SEVERITIES),
  module: new Set(MODULES),
  rule: null,
  file: null,
  query: "",
  limit: PAGE_SIZE,
  selected: -1,
};

function h(tag, props, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === "class") node.className = value;
    else if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
    else if (key in node && key !== "list") node[key] = value;
    else node.setAttribute(key, value === true ? "" : value);
  }
  node.append(...children.flat().filter((child) => child !== null && child !== undefined && child !== false));
  return node;
}

function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", "#i-" + name);
  svg.append(use);
  return svg;
}

function parseTarget(input, explicitRef) {
  const text = input.trim().replace(/\.git$/, "").replace(/\/+$/, "");
  const match = text.match(/^(?:https?:\/\/)?(?:www\.)?(?:github\.com\/)?([\w.-]+)\/([\w.-]+)(?:\/(?:tree|blob|commit)\/(.+))?(?:@(.+))?$/);
  if (!match) return null;
  const [, owner, name, pathRef, atRef] = match;
  return { repo: `${owner}/${name}`, ref: explicitRef.trim() || atRef || pathRef || "" };
}

function targetFromHash() {
  const value = decodeURIComponent(location.hash.slice(1));
  if (!value) return null;
  const [repo, ref = ""] = value.split("@");
  return parseTarget(repo, ref);
}

function startScan(target) {
  $("repo").value = target.repo;
  $("ref").value = target.ref;
  const hash = "#" + target.repo + (target.ref ? "@" + target.ref : "");
  if (location.hash !== hash) history.replaceState(null, "", hash);
  document.title = `${target.repo} · slopcop`;

  $("error").hidden = true;
  $("results").hidden = true;
  $("status").hidden = false;
  $("scan-button").disabled = true;
  setProgress("Loading scanner", 0, 0);
  worker.postMessage({ ...target, token: $("token").value.trim() });
}

function setProgress(stage, done, total) {
  $("status-text").textContent = stage + "…";
  $("status-count").textContent = total ? `${done} / ${total}` : "";
  const bar = $("progress-bar");
  bar.classList.toggle("indeterminate", !total);
  bar.style.width = total ? `${(100 * done) / total}%` : "";
}

function issueUrl(template, fields) {
  const params = new URLSearchParams({ template });
  for (const [key, value] of Object.entries(fields)) {
    if (value) params.set(key, value.length > MAX_FIELD ? value.slice(0, MAX_FIELD) + "\n…" : value);
  }
  return `${REPO_URL}/issues/new?${params}`;
}

function versionLabel() {
  const version = state.data?.result.version;
  return version ? `slopcop ${version} (web demo)` : "slopcop web demo";
}

function bugReportUrl(actual = "") {
  return issueUrl("bug_report.yml", {
    version: versionLabel(),
    environment: `Web demo in ${navigator.userAgent}`,
    reproduction: state.data || actual ? `Scan ${location.href}` : "",
    actual,
  });
}

function falsePositiveUrl(finding) {
  const { line, column } = finding.location;
  const output = [`${finding.path}:${line}:${column}  ${finding.rule_id}  ${finding.severity}`, finding.message];
  if (finding.observation) output.push(`observed: ${finding.observation}`);
  return issueUrl("false_positive.yml", {
    title: `false positive: ${finding.rule_id} in ${finding.path}`,
    rule: finding.rule_id,
    version: versionLabel(),
    context: `${finding.path} at ${blobUrl(finding.path, line)}`,
    snippet: snippetLines(finding)?.map((row) => row.text).join("\n") || finding.evidence || "",
    finding: output.join("\n"),
  });
}

function missedSlopUrl() {
  return issueUrl("false_negative.yml", { version: versionLabel() });
}

function nominateUrl(finding) {
  const { line, column } = finding.location;
  const output = [`${finding.path}:${line}:${column}  ${finding.rule_id}  ${finding.severity}`, finding.message];
  if (finding.observation) output.push(`observed: ${finding.observation}`);
  return issueUrl("most_wanted.yml", {
    title: `most wanted: ${finding.rule_id}`,
    rule: finding.rule_id,
    version: versionLabel(),
    context: finding.path,
    snippet: snippetLines(finding)?.map((row) => row.text).join("\n") || finding.evidence || "",
    finding: output.join("\n"),
    source: blobUrl(finding.path, line),
  });
}

function showError(message, reportable = false) {
  $("error").replaceChildren(message);
  if (reportable) {
    $("error").append(" ", h("a", { href: bugReportUrl(message), target: "_blank", rel: "noopener" }, "Report this as a bug"), ".");
  }
  $("error").hidden = false;
}

worker.onmessage = ({ data }) => {
  if (data.type === "rules") {
    renderCatalog(data.rules);
    return;
  }
  if (data.type === "progress") {
    setProgress(data.stage, data.done, data.total);
    return;
  }
  $("status").hidden = true;
  $("scan-button").disabled = false;
  if (data.type === "error") {
    showError(data.message, !data.code);
    if (data.code === "bad-token" || (data.code === "rate-limit" && !$("token").value.trim())) {
      $("token-panel").open = true;
      $("token").focus();
    }
    return;
  }
  showResults(data);
};

function renderCatalog(rules) {
  $("rule-total").textContent = `${rules.length} rules across ${MODULES.length} modules.`;
  for (const card of document.querySelectorAll(".module")) {
    const module = card.dataset.module;
    const members = rules.filter((rule) => rule.module === module);
    card.querySelector(".module-count").textContent = `${members.length} ${members.length === 1 ? "rule" : "rules"}`;
    const extra = members.length - SAMPLE_RULES;
    card.querySelector(".module-rules").replaceChildren(
      ...members.slice(0, SAMPLE_RULES).map((rule) => h("li", { title: rule.message }, h("code", {}, rule.id), " ", rule.description)),
      ...(extra > 0 ? [h("li", { class: "module-more" }, h("a", { href: `${RULES_DOC}#${module}`, target: "_blank", rel: "noopener" }, `${extra} more`, icon("arrow")))] : []),
    );
  }
}

worker.onerror = (event) => {
  $("status").hidden = true;
  $("scan-button").disabled = false;
  showError(`The scanner failed to load: ${event.message || "unknown error"}.`, true);
};

function showResults(data) {
  state.data = data;
  state.rules = new Map(data.rules.map((rule) => [rule.id, rule]));
  state.severity = new Set(SEVERITIES);
  state.module = new Set(MODULES);
  state.rule = null;
  state.file = null;
  state.query = "";
  state.limit = PAGE_SIZE;
  state.selected = -1;
  $("query").value = "";

  renderSummary();
  $("notices").replaceChildren(...data.notices.map((notice) => h("li", {}, notice)));
  renderFacets();
  renderFindings();
  document.body.classList.add("has-results");
  $("results").hidden = false;
}

function renderSummary() {
  const { repo, ref, sha, result, stats } = state.data;
  const counts = countBy(result.findings, (finding) => finding.severity);
  const treeUrl = `https://github.com/${repo}/tree/${sha}`;
  const download = (content, type, extension) => () => {
    const blob = new Blob([content], { type });
    const link = h("a", { href: URL.createObjectURL(blob), download: `slopcop-${repo.replace("/", "-")}.${extension}` });
    link.click();
    URL.revokeObjectURL(link.href);
  };

  $("summary").replaceChildren(
    h("div", { class: "summary-target" },
      h("a", { href: treeUrl, class: "summary-repo" }, repo),
      h("span", { class: "summary-ref" }, (ref ? ref + " · " : "") + sha.slice(0, 7)),
    ),
    h("dl", { class: "summary-stats" },
      stat(result.summary.findings, result.summary.findings === 1 ? "finding" : "findings", "total"),
      ...SEVERITIES.map((severity) => stat(counts.get(severity) || 0, severity, severity)),
      stat(result.summary.scanned_files, "files scanned"),
      stat(result.summary.skipped_files, "skipped"),
      stat(formatMillis(stats.totalMillis), `total, ${formatMillis(stats.scanMillis)} scanning`),
    ),
    h("div", { class: "summary-actions" },
      h("button", { type: "button", class: "secondary", onclick: download(state.data.html, "text/html", "html") }, icon("download"), "Download HTML"),
      h("button", { type: "button", class: "secondary", onclick: download(JSON.stringify(result, null, 2), "application/json", "json") }, icon("download"), "Download JSON"),
      h("a", { class: "secondary", href: missedSlopUrl(), target: "_blank", rel: "noopener", title: "Open a prefilled missed-slop issue on GitHub" }, icon("search-x"), "Report missed slop")),
  );
}

function stat(value, label, tone) {
  return h("div", { class: "stat" + (tone ? " stat-" + tone : "") }, h("dt", {}, String(value)), h("dd", {}, label));
}

function formatMillis(millis) {
  return millis < 1000 ? `${Math.round(millis)} ms` : `${(millis / 1000).toFixed(1)} s`;
}

function countBy(items, key) {
  const counts = new Map();
  for (const item of items) counts.set(key(item), (counts.get(key(item)) || 0) + 1);
  return counts;
}

function renderFacets() {
  const findings = state.data.result.findings;
  const severityCounts = countBy(findings, (finding) => finding.severity);
  const moduleCounts = countBy(findings, (finding) => finding.module);

  $("facet-severity").replaceChildren(...SEVERITIES.map((severity) =>
    toggleChip(severity, severityCounts.get(severity) || 0, state.severity, "sev-" + severity)));
  $("facet-module").replaceChildren(...MODULES.filter((name) => moduleCounts.has(name)).map((name) =>
    toggleChip(name, moduleCounts.get(name), state.module)));

  const ruleCounts = [...countBy(findings, (finding) => finding.rule_id)].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  $("facet-rule").replaceChildren(...ruleCounts.map(([id, count]) =>
    facetItem(id, state.rules.get(id)?.description || "", count, state.rule === id, () => {
      state.rule = state.rule === id ? null : id;
      refresh();
    })));

  const fileCounts = [...countBy(findings, (finding) => finding.path)].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  $("facet-file").replaceChildren(...fileCounts.map(([path, count]) =>
    facetItem(path, "", count, state.file === path, () => {
      state.file = state.file === path ? null : path;
      refresh();
    })));
}

function toggleChip(label, count, set, extraClass = "") {
  return h("button", {
    type: "button",
    class: `chip ${extraClass}`,
    "aria-pressed": String(set.has(label)),
    onclick: () => {
      if (set.has(label)) set.delete(label);
      else set.add(label);
      refresh();
    },
  }, label, h("span", { class: "count" }, String(count)));
}

function facetItem(label, title, count, active, onclick) {
  return h("li", {},
    h("button", { type: "button", class: "facet-item", title: title || label, "aria-pressed": String(active), onclick },
      h("span", { class: "facet-label" }, label),
      h("span", { class: "count" }, String(count))));
}

function refresh() {
  state.limit = PAGE_SIZE;
  state.selected = -1;
  renderFacets();
  renderFindings();
}

function matches(finding) {
  if (!state.severity.has(finding.severity) || !state.module.has(finding.module)) return false;
  if (state.rule && finding.rule_id !== state.rule) return false;
  if (state.file && finding.path !== state.file) return false;
  if (!state.query) return true;
  const haystack = [finding.path, finding.rule_id, finding.message, finding.evidence, finding.observation].join("\n").toLowerCase();
  return state.query.split(/\s+/).every((term) => haystack.includes(term));
}

function renderFindings() {
  const all = state.data.result.findings;
  const visible = all.filter(matches);
  const filtered = visible.length !== all.length;
  $("match-count").textContent = filtered ? `${visible.length} of ${all.length} findings` : `${all.length} findings`;
  $("clear-filters").hidden = !filtered;

  const container = $("findings");
  if (all.length === 0) {
    container.replaceChildren(h("div", { class: "empty" },
      h("strong", {}, "No findings."),
      ` slopcop scanned ${state.data.result.summary.scanned_files} files and found nothing to report. Missed something? `,
      h("a", { href: missedSlopUrl(), target: "_blank", rel: "noopener" }, "Report missed slop"), "."));
    return;
  }
  if (visible.length === 0) {
    container.replaceChildren(h("div", { class: "empty" }, "No findings match the current filters."));
    return;
  }

  const groups = [];
  for (const finding of visible.slice(0, state.limit)) {
    const last = groups[groups.length - 1];
    if (last && last.path === finding.path) last.findings.push(finding);
    else groups.push({ path: finding.path, findings: [finding] });
  }

  container.replaceChildren(...groups.map(renderGroup));
  if (visible.length > state.limit) {
    container.append(h("button", { type: "button", class: "secondary more", onclick: () => { state.limit += PAGE_SIZE; renderFindings(); } },
      `Show ${Math.min(PAGE_SIZE, visible.length - state.limit)} more of ${visible.length - state.limit} remaining`));
  }
  if (state.selected >= 0) select(state.selected, false);
}

function blobUrl(path, line) {
  const { repo, sha } = state.data;
  return `https://github.com/${repo}/blob/${sha}/${path.split("/").map(encodeURIComponent).join("/")}` + (line ? `#L${line}` : "");
}

function renderGroup(group) {
  return h("section", { class: "file-group" },
    h("header", { class: "file-header" },
      h("button", { type: "button", class: "file-path", title: "Show only this file", onclick: () => { state.file = group.path; refresh(); } }, group.path),
      h("span", { class: "count" }, String(group.findings.length)),
      h("a", { href: blobUrl(group.path), target: "_blank", rel: "noopener" }, "GitHub", icon("external"))),
    ...group.findings.map(renderFinding));
}

function renderFinding(finding) {
  const rule = state.rules.get(finding.rule_id);
  const { line, column } = finding.location;
  const explanation = rule ? renderExplanation(rule) : null;
  const toggleExplanation = () => {
    if (explanation) explanation.hidden = !explanation.hidden;
  };

  return h("article", { class: `finding sev-${finding.severity}`, tabindex: "-1", onclick: (event) => {
    const cards = [...document.querySelectorAll(".finding")];
    select(cards.indexOf(event.currentTarget), false);
  } },
    h("div", { class: "finding-head" },
      h("span", { class: `badge sev-${finding.severity}` }, finding.severity),
      h("button", { type: "button", class: "rule-id", title: "Explain this rule (e)", onclick: toggleExplanation }, finding.rule_id),
      h("span", { class: "finding-message" }, finding.message),
      h("a", { class: "location", href: blobUrl(finding.path, line), target: "_blank", rel: "noopener", title: "Open on GitHub (o)" }, `${line}:${column}`)),
    finding.observation ? h("p", { class: "observation" }, finding.observation) : null,
    renderSnippet(finding),
    h("div", { class: "finding-foot" },
      finding.suggestion ? h("p", { class: "suggestion" }, h("span", {}, "Fix: "), finding.suggestion) : null,
      h("span", { class: "finding-actions" },
        h("a", { class: "nominate", href: nominateUrl(finding), target: "_blank", rel: "noopener", title: "Nominate this finding for the Most Wanted gallery (w)" }, icon("crosshair"), "Nominate"),
        h("a", { class: "report-false-positive", href: falsePositiveUrl(finding), target: "_blank", rel: "noopener", title: "Open a prefilled false-positive issue on GitHub (f)" }, icon("flag"), "Report false positive"))),
    explanation);
}

function snippetLines(finding) {
  const source = state.data.sources[finding.path];
  if (source === undefined) return null;
  const lines = source.split("\n");
  const target = finding.location.line;
  const first = Math.max(1, target - CONTEXT_LINES);
  const last = Math.min(lines.length, target + CONTEXT_LINES);
  const rows = [];
  for (let number = first; number <= last; number += 1) {
    rows.push({ number, hit: number === target, text: lines[number - 1].replace(/\r$/, "") });
  }
  return rows;
}

function renderSnippet(finding) {
  const rows = snippetLines(finding);
  if (rows === null) {
    return finding.evidence ? h("pre", { class: "evidence" }, finding.evidence) : null;
  }
  return h("div", { class: "code" }, rows.map((row) =>
    h("div", { class: "code-line" + (row.hit ? " hit" : "") },
      h("span", { class: "gutter" }, String(row.number)),
      h("span", { class: "text" }, row.text || " "))));
}

function renderExplanation(rule) {
  return h("div", { class: "explanation", hidden: true },
    h("p", { class: "explanation-title" }, h("strong", {}, rule.id), ` · ${rule.module} · default ${rule.default_severity}, ${rule.default_confidence} confidence`),
    h("p", {}, rule.description),
    h("h3", {}, "Why it matters"),
    h("p", {}, rule.rationale),
    rule.examples.length ? [h("h3", {}, "Example"), ...rule.examples.map((example) => h("pre", {}, example))] : null,
    h("h3", {}, "False positives"),
    h("p", {}, rule.false_positives),
    h("p", { class: "explanation-hint" }, "Suppress with a reason: ", h("code", {}, `slopcop: ignore ${rule.id} -- reason`)));
}

function select(index, scroll = true) {
  const cards = [...document.querySelectorAll(".finding")];
  if (cards.length === 0) return;
  state.selected = Math.max(0, Math.min(index, cards.length - 1));
  for (const card of cards) card.classList.remove("selected");
  const card = cards[state.selected];
  card.classList.add("selected");
  if (scroll) {
    card.scrollIntoView({ block: "nearest", behavior: "smooth" });
    card.focus({ preventScroll: true });
  }
}

document.addEventListener("keydown", (event) => {
  if (!state.data || $("results").hidden || event.metaKey || event.ctrlKey || event.altKey) return;
  const typing = event.target.matches("input, textarea");
  if (event.key === "Escape" && event.target === $("query")) {
    $("query").blur();
    return;
  }
  if (typing) return;
  const card = document.querySelectorAll(".finding")[state.selected];
  switch (event.key) {
    case "j": select(state.selected + 1); break;
    case "k": select(state.selected - 1); break;
    case "o": card?.querySelector(".location").click(); break;
    case "e": card?.querySelector(".rule-id").click(); break;
    case "f": card?.querySelector(".report-false-positive").click(); break;
    case "w": card?.querySelector(".nominate").click(); break;
    case "/": event.preventDefault(); $("query").focus(); break;
    default: return;
  }
});

$("query").addEventListener("input", (event) => {
  state.query = event.target.value.trim().toLowerCase();
  state.limit = PAGE_SIZE;
  state.selected = -1;
  renderFindings();
});

$("clear-filters").addEventListener("click", () => {
  state.severity = new Set(SEVERITIES);
  state.module = new Set(MODULES);
  state.rule = null;
  state.file = null;
  state.query = "";
  $("query").value = "";
  refresh();
});

$("scan-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const target = parseTarget($("repo").value, $("ref").value);
  if (!target) {
    showError("Enter a repository as owner/name or a github.com URL.");
    return;
  }
  startScan(target);
});

function saveToken() {
  const token = $("token").value.trim();
  try {
    if (token) sessionStorage.setItem(TOKEN_KEY, token);
    else sessionStorage.removeItem(TOKEN_KEY);
  } catch {
    // Storage can be blocked by the browser; the token then lasts only until the page closes.
  }
  $("token-state").textContent = token ? "(set)" : "(optional)";
}

$("token").addEventListener("input", saveToken);

$("forget-token").addEventListener("click", () => {
  $("token").value = "";
  saveToken();
});

try {
  $("token").value = sessionStorage.getItem(TOKEN_KEY) || "";
} catch {
  // Without storage the field starts empty and scans run anonymously.
}
saveToken();

$("report-bug").addEventListener("click", (event) => {
  event.currentTarget.href = bugReportUrl();
});

$("report-missed").addEventListener("click", (event) => {
  event.currentTarget.href = missedSlopUrl();
});

const installTabs = [...document.querySelectorAll(".terminal-tabs [role=tab]")];

function selectInstallTab(tab) {
  for (const other of installTabs) {
    const selected = other === tab;
    other.setAttribute("aria-selected", String(selected));
    other.tabIndex = selected ? 0 : -1;
    $(other.getAttribute("aria-controls")).hidden = !selected;
  }
}

for (const tab of installTabs) {
  tab.addEventListener("click", () => selectInstallTab(tab));
  tab.addEventListener("keydown", (event) => {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[event.key];
    if (!step) return;
    const next = installTabs[(installTabs.indexOf(tab) + step + installTabs.length) % installTabs.length];
    selectInstallTab(next);
    next.focus();
  });
}

if (/Windows/.test(navigator.userAgent)) selectInstallTab($("tab-windows"));

$("copy-install").addEventListener("click", async (event) => {
  const button = event.currentTarget;
  const panel = document.querySelector(".terminal [role=tabpanel]:not([hidden])").cloneNode(true);
  for (const prompt of panel.querySelectorAll(".prompt")) prompt.remove();
  try {
    await navigator.clipboard.writeText(panel.textContent);
  } catch {
    return;
  }
  button.replaceChildren(icon("check"));
  setTimeout(() => button.replaceChildren(icon("copy")), 1500);
});

const SPECIMEN_INTERVAL = 7000;
const specimenTabs = [...document.querySelectorAll(".specimen-tabs [role=tab]")];
const specimen = { index: 0, paused: matchMedia("(prefers-reduced-motion: reduce)").matches, held: false, timer: 0 };

function showSpecimen(index) {
  specimen.index = (index + specimenTabs.length) % specimenTabs.length;
  specimenTabs.forEach((tab, i) => {
    const selected = i === specimen.index;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
    const slide = $(tab.getAttribute("aria-controls"));
    slide.classList.toggle("active", selected);
    if (selected) $("specimen-file").textContent = slide.dataset.file;
  });
}

function scheduleSpecimen() {
  clearTimeout(specimen.timer);
  const running = !specimen.paused && !specimen.held && !document.hidden
    && !document.body.classList.contains("has-results");
  $("specimen-slides").setAttribute("aria-live", running ? "off" : "polite");
  if (running) specimen.timer = setTimeout(() => {
    showSpecimen(specimen.index + 1);
    scheduleSpecimen();
  }, SPECIMEN_INTERVAL);
}

function setSpecimenPaused(paused) {
  specimen.paused = paused;
  const button = $("specimen-pause");
  button.setAttribute("aria-label", paused ? "Play examples" : "Pause examples");
  button.replaceChildren(icon(paused ? "play" : "pause"));
  scheduleSpecimen();
}

specimenTabs.forEach((tab, i) => {
  tab.addEventListener("click", () => {
    showSpecimen(i);
    setSpecimenPaused(true);
  });
  tab.addEventListener("keydown", (event) => {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[event.key];
    if (!step) return;
    showSpecimen(specimen.index + step);
    specimenTabs[specimen.index].focus();
    setSpecimenPaused(true);
  });
});

$("specimen-pause").addEventListener("click", () => setSpecimenPaused(!specimen.paused));
for (const [type, held] of [["mouseenter", true], ["mouseleave", false], ["focusin", true], ["focusout", false]]) {
  $("specimen").addEventListener(type, () => {
    specimen.held = held;
    scheduleSpecimen();
  });
}
document.addEventListener("visibilitychange", scheduleSpecimen);
setSpecimenPaused(specimen.paused);

$("examples").addEventListener("click", (event) => {
  const repo = event.target.closest("button")?.dataset.repo;
  if (repo) startScan({ repo, ref: "" });
});

window.addEventListener("hashchange", () => {
  const target = targetFromHash();
  if (target && !$("scan-button").disabled) startScan(target);
});

if (matchMedia("(max-width: 860px)").matches) {
  for (const facet of document.querySelectorAll(".collapsible")) facet.open = false;
}

worker.postMessage({ type: "rules" });

const initial = targetFromHash();
if (initial) startScan(initial);
