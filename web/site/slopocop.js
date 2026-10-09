// slopocop is an opt-in persona. Typing "slopocop" outside a text field toggles it, and
// `?slopocop=on` or `?slopocop=off` sets it on devices without a keyboard. The choice is kept per
// browser; every line here is fixed text, so reports stay the same with the persona on or off.

const STORAGE_KEY = "slopcop.slopocop";
const CODE = "slopocop";
const STAGES = {
  "Loading scanner": "Booting slopocop",
  "Listing files": "Sweeping the area",
  "Downloading files": "Bringing files in",
  "Downloading Polygraph embedding model": "Fetching the small detector",
  "Downloading Polygraph tokenizer": "Fetching the tokenizer",
  "Downloading Polygraph language model": "Fetching the large detector",
  Scanning: "Targeting",
  "Scanning with the Polygraph language model": "Interrogating",
};

const listeners = [];
let enabled = initial();

function initial() {
  const requested = new URLSearchParams(location.search).get(CODE);
  if (requested === "on" || requested === "off") {
    store(requested === "on");
    return requested === "on";
  }
  try {
    return localStorage.getItem(STORAGE_KEY) === "on";
  } catch {
    // Blocked storage leaves the persona off, which is the default.
    return false;
  }
}

function store(value) {
  try {
    if (value) localStorage.setItem(STORAGE_KEY, "on");
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Without storage the persona lasts until the page closes.
  }
}

export const slopocop = {
  get enabled() {
    return enabled;
  },

  set(value) {
    enabled = value;
    store(value);
    apply();
    for (const listener of listeners) listener(value);
  },

  onChange(listener) {
    listeners.push(listener);
  },

  stage(text) {
    return (enabled && STAGES[text]) || text;
  },

  verdict(findings) {
    if (findings === 0) return "Area secure. Thank you for your cooperation.";
    return `${findings} ${findings === 1 ? "suspect" : "suspects"} in custody. Your move.`;
  },
};

function apply() {
  document.body.classList.toggle("slopocop", enabled);
  document.querySelector(".brand span").textContent = enabled ? "slopocop" : "slopcop";
  document.querySelector('link[rel="icon"]').href = enabled ? "slopocop.svg" : ORIGINAL_ICON;
}

const ORIGINAL_ICON = document.querySelector('link[rel="icon"]').href;

function announce(value) {
  const toast = document.getElementById("slopocop-toast");
  toast.textContent = value ? "slopocop online. Directives loaded." : "slopocop offline.";
  toast.hidden = false;
  clearTimeout(announce.timer);
  announce.timer = setTimeout(() => {
    toast.hidden = true;
  }, 2500);
}

// Registered before the results shortcuts, so once "sl" is typed the rest of the code (which
// includes the `o` shortcut) does not reach them.
let matched = 0;
document.addEventListener("keydown", (event) => {
  if (event.metaKey || event.ctrlKey || event.altKey || event.target.matches("input, textarea")) return;
  const key = event.key.toLowerCase();
  if (key === CODE[matched]) matched += 1;
  else matched = key === CODE[0] ? 1 : 0;
  if (matched < 2) return;
  event.stopImmediatePropagation();
  if (matched < CODE.length) return;
  matched = 0;
  slopocop.set(!enabled);
  announce(enabled);
});

apply();
