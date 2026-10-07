// Interactive parts of the API reference: live requests to the JSON endpoints, runnable module
// examples, copy buttons, and the menu entry for the section in view.

const $ = (id) => document.getElementById(id);
let module;
const api = () => (module ??= import("./v1/slopcop.js"));

function show(output, text) {
  output.hidden = false;
  output.querySelector("code").textContent = text;
}

// Shows the request's live response below the example.
for (const box of document.querySelectorAll("[data-try]")) {
  const output = box.parentElement.querySelector(".response") ?? box.insertAdjacentElement("afterend", Object.assign(document.createElement("pre"), { className: "response", innerHTML: "<code></code>" }));
  box.querySelector(".run").addEventListener("click", async () => {
    const path = box.dataset.try.replace("{id}", box.querySelector("select")?.value ?? "");
    const started = performance.now();
    try {
      const response = await fetch(path, { cache: "no-cache" });
      const body = await response.text();
      const elapsed = Math.round(performance.now() - started);
      let text = body;
      try {
        text = JSON.stringify(JSON.parse(body), null, 2);
      } catch {
        // A 404 page is HTML; show its status only.
        text = "";
      }
      show(output, `// ${response.status} ${response.statusText} · ${elapsed} ms · GET ${new URL(path, location).pathname}\n${text}`);
    } catch (error) {
      show(output, `// Request failed: ${error.message}`);
    }
  });
}

// Fills the rule picker from the catalog.
fetch("v1/rules.json")
  .then((response) => response.json())
  .then((rules) => {
    const select = $("rule-id");
    select.replaceChildren(...rules.map((rule) => new Option(`${rule.id} · ${rule.description}`, rule.id, false, rule.id === "VIBE013")));
  })
  .catch(() => {});

$("play-run").addEventListener("click", async (event) => {
  const output = $("play-output");
  const button = event.currentTarget;
  button.disabled = true;
  try {
    const { scanFiles } = await api();
    const report = await scanFiles({ [$("play-path").value.trim() || "file.txt"]: $("play-source").value });
    show(output, JSON.stringify(report, null, 2));
  } catch (error) {
    show(output, `// ${error.code ? error.code + ": " : ""}${error.message}`);
  } finally {
    button.disabled = false;
  }
});

$("repo-run").addEventListener("click", async (event) => {
  const output = $("repo-output");
  const button = event.currentTarget;
  button.disabled = true;
  show(output, "// Loading the module");
  try {
    const { scanRepository } = await api();
    const scan = await scanRepository($("repo-input").value, {
      onProgress: ({ stage, done, total }) => show(output, `// ${stage}${total ? ` ${done}/${total}` : ""}`),
    });
    const { repo, ref, sha, report, notices, stats } = scan;
    const preview = {
      repo,
      ref,
      sha,
      notices,
      stats,
      report: { version: report.version, summary: report.summary, findings: [...report.findings.slice(0, 3), ...(report.findings.length > 3 ? [`… ${report.findings.length - 3} more`] : [])] },
      html: `${scan.html.length.toLocaleString()} characters`,
    };
    show(output, JSON.stringify(preview, null, 2));
  } catch (error) {
    show(output, `// ${error.code ? error.code + ": " : ""}${error.message}`);
  } finally {
    button.disabled = false;
  }
});

for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.querySelector("use").setAttribute("href", "#i-check");
      setTimeout(() => button.querySelector("use").setAttribute("href", "#i-copy"), 1500);
    } catch {
      // Clipboard access can be denied; the URL is also in the example below.
    }
  });
}

// Marks the menu entry of the section nearest the top of the viewport.
const links = new Map([...document.querySelectorAll(".subnav a")].map((link) => [link.hash.slice(1), link]));
const visible = new Set();
const observer = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (entry.isIntersecting) visible.add(entry.target);
    else visible.delete(entry.target);
  }
  const current = [...links.keys()].find((id) => visible.has($(id)));
  if (!current) return;
  for (const [id, link] of links) {
    if (id === current) {
      link.setAttribute("aria-current", "location");
      // On narrow screens the menu scrolls sideways; keep the current entry in it.
      const menu = link.parentElement;
      menu.scrollLeft = link.offsetLeft - (menu.clientWidth - link.offsetWidth) / 2;
    } else {
      link.removeAttribute("aria-current");
    }
  }
}, { rootMargin: "-80px 0px -55% 0px" });
for (const id of links.keys()) observer.observe($(id));
