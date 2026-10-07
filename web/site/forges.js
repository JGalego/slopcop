// A repository is named `owner/name` on GitHub and `host/path` elsewhere, such as
// `gitlab.com/group/project` or `codeberg.org/owner/name`. Each forge resolves a ref to a commit,
// lists the commit's files, downloads them, and links to them on its website.

// GitLab and Codeberg do not expose their rate-limit headers to the page, so a throttled request
// waits this long before it is retried.
const THROTTLE_WAIT_MS = 60_000;
const THROTTLE_RETRIES = 3;

const encodePath = (path) => path.split("/").map(encodeURIComponent).join("/");

const github = {
  name: "GitHub",
  host: "github.com",
  // Every blob comes from raw.githubusercontent.com, which is not counted against the API limit.
  maxFiles: 3000,
  concurrency: 24,
  treeUrl: (path, sha) => `https://github.com/${path}/tree/${sha}`,
  blobBase: (path, sha) => `https://github.com/${path}/blob/${sha}/`,

  async resolve(path, ref, token) {
    return (await githubApi(`/repos/${path}/commits/${encodeURIComponent(ref || "HEAD")}`, token, "application/vnd.github.sha")).trim();
  },

  async list(path, sha, token) {
    const tree = await githubApi(`/repos/${path}/git/trees/${sha}?recursive=1`, token);
    return {
      blobs: tree.tree.filter((entry) => entry.type === "blob").map(({ path, size }) => ({ path, size })),
      truncated: tree.truncated,
    };
  },

  raw: (path, sha, file) => download(`https://raw.githubusercontent.com/${path}/${sha}/${encodePath(file)}`),
};

const gitlab = {
  name: "GitLab",
  host: "gitlab.com",
  // Anonymous clients get 500 API requests per minute, and every blob is one request, so a scan
  // stays under the limit with room for the tree pages.
  maxFiles: 400,
  concurrency: 8,
  maxPages: 50,
  treeUrl: (path, sha) => `https://gitlab.com/${path}/-/tree/${sha}`,
  blobBase: (path, sha) => `https://gitlab.com/${path}/-/blob/${sha}/`,
  api: (path) => `https://gitlab.com/api/v4/projects/${encodeURIComponent(path)}`,

  async resolve(path, ref) {
    const commit = await (await forgeApi(this, `${this.api(path)}/repository/commits/${encodeURIComponent(ref || "HEAD")}`)).json();
    return commit.id;
  },

  // The tree lists 100 entries per page and leaves out blob sizes.
  async list(path, sha) {
    const blobs = [];
    let url = `${this.api(path)}/repository/tree?ref=${sha}&recursive=true&per_page=100&pagination=keyset`;
    for (let page = 0; url && page < this.maxPages; page += 1) {
      const response = await forgeApi(this, url);
      for (const entry of await response.json()) {
        if (entry.type === "blob") blobs.push({ path: entry.path });
      }
      url = response.headers.get("link")?.match(/<([^>]+)>;\s*rel="next"/)?.[1];
    }
    return { blobs, truncated: Boolean(url) };
  },

  raw(path, sha, file) {
    return download(`${this.api(path)}/repository/files/${encodeURIComponent(file)}/raw?ref=${sha}`, this);
  },
};

const codeberg = {
  name: "Codeberg",
  host: "codeberg.org",
  // Anonymous clients get 2,000 API requests per 10 minutes, and every blob is one request.
  maxFiles: 1000,
  concurrency: 8,
  maxPages: 20,
  treeUrl: (path, sha) => `https://codeberg.org/${path}/src/commit/${sha}`,
  blobBase: (path, sha) => `https://codeberg.org/${path}/src/commit/${sha}/`,
  api: (path) => `https://codeberg.org/api/v1/repos/${path}`,

  async resolve(path, ref) {
    const query = new URLSearchParams({ limit: "1", stat: "false", verification: "false", files: "false" });
    if (ref) query.set("sha", ref);
    const [commit] = await (await forgeApi(this, `${this.api(path)}/commits?${query}`)).json();
    if (!commit) throw notFound();
    return commit.sha;
  },

  // The tree lists up to 1,000 entries per page and marks every page but the last as truncated.
  async list(path, sha) {
    const blobs = [];
    let truncated = true;
    for (let page = 1; truncated && page <= this.maxPages; page += 1) {
      const tree = await (await forgeApi(this, `${this.api(path)}/git/trees/${sha}?recursive=true&per_page=1000&page=${page}`)).json();
      for (const entry of tree.tree ?? []) {
        if (entry.type === "blob") blobs.push({ path: entry.path, size: entry.size });
      }
      truncated = tree.truncated && (tree.tree ?? []).length > 0;
    }
    return { blobs, truncated };
  },

  raw(path, sha, file) {
    return download(`${this.api(path)}/raw/${encodePath(file)}?ref=${sha}`, this);
  },
};

const FORGES = [github, gitlab, codeberg];

// Splits a repository name into its forge and the path on that forge.
export function forgeOf(repo) {
  const [host, ...rest] = repo.split("/");
  const forge = FORGES.find((candidate) => candidate !== github && candidate.host === host.toLowerCase());
  return forge ? { forge, path: rest.join("/") } : { forge: github, path: repo };
}

// Reads a repository name or web URL, with an optional `@ref` or a ref taken from the URL.
export function parseRepository(input) {
  const text = input.trim().replace(/^(?:https?:\/\/)?(?:www\.)?/i, "").replace(/\.git$/, "").replace(/\/+$/, "");
  const [, location, atRef = ""] = text.match(/^([^@]+)(?:@(.+))?$/) ?? [];
  if (!location) return null;
  const segment = "[\\w.-]+";
  const patterns = [
    [gitlab, new RegExp(`^gitlab\\.com/(${segment}(?:/${segment})+?)(?:/-/(?:tree|blob|commit)/(.+))?$`, "i")],
    [codeberg, new RegExp(`^codeberg\\.org/(${segment}/${segment})(?:/(?:src/(?:branch|tag|commit)|commit)/(.+))?$`, "i")],
    [github, new RegExp(`^(?:github\\.com/)?(${segment}/${segment})(?:/(?:tree|blob|commit)/(.+))?$`, "i")],
  ];
  // A gitlab.com or codeberg.org address that does not name a repository is not a GitHub one either.
  const host = location.split("/")[0].toLowerCase();
  const [forge, pattern] = patterns.find(([candidate]) => candidate.host === host) ?? patterns[2];
  const match = location.match(pattern);
  if (!match) return null;
  return { repo: forge === github ? match[1] : `${forge.host}/${match[1]}`, ref: atRef || match[2] || "" };
}

const notFound = () => Object.assign(new Error("Repository or ref not found. Only public repositories can be scanned."), { code: "not-found" });

async function githubApi(path, token, accept = "application/vnd.github+json") {
  const headers = { Accept: accept };
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }
  const response = await fetch("https://api.github.com" + path, { headers });
  if (response.ok) {
    return accept.endsWith("sha") ? response.text() : response.json();
  }
  if (response.status === 401 && token) {
    throw Object.assign(new Error("GitHub rejected the token. It may have expired or been revoked; replace it or select Forget to scan anonymously."), { code: "bad-token" });
  }
  if ((response.status === 403 || response.status === 429) && response.headers.get("x-ratelimit-remaining") === "0") {
    const reset = new Date(Number(response.headers.get("x-ratelimit-reset")) * 1000).toLocaleTimeString();
    const message = token
      ? `Your token's GitHub API limit is used up. It resets at ${reset}.`
      : `GitHub's anonymous API limit (60 requests per hour) is used up for your network. It resets at ${reset}. Add a GitHub token to raise the limit to 5,000 requests per hour.`;
    throw Object.assign(new Error(message), { code: "rate-limit" });
  }
  if (response.status === 404 || response.status === 422) {
    throw notFound();
  }
  throw new Error(`GitHub responded with ${response.status} ${response.statusText}.`);
}

// Requests sent to GitLab and Codeberg carry no headers. Codeberg does not answer CORS preflight
// requests, and a plain GET needs none.
async function forgeApi(forge, url) {
  const response = await throttled(forge, url);
  if (response.ok) {
    return response;
  }
  if (response.status === 404) {
    throw notFound();
  }
  if (response.status === 429) {
    throw Object.assign(new Error(`${forge.name}'s anonymous API limit is used up for your network. Try again in a few minutes.`), { code: "host-limit" });
  }
  throw new Error(`${forge.name} responded with ${response.status} ${response.statusText}.`);
}

let throttleListener = () => {};

// Calls `listener(forge, milliseconds)` whenever a forge throttles the scan and every request waits.
export function onThrottle(listener) {
  throttleListener = listener;
}

// Retries a throttled request after a pause, and makes every other request to the forge wait too.
const pausedUntil = new Map();
async function throttled(forge, url, retries = THROTTLE_RETRIES) {
  const wait = (pausedUntil.get(forge) ?? 0) - Date.now();
  if (wait > 0) {
    await new Promise((resolve) => setTimeout(resolve, wait));
  }
  const response = await fetch(url);
  if (response.status !== 429 || retries === 0) {
    return response;
  }
  const retryAfter = Number(response.headers.get("retry-after")) * 1000 || THROTTLE_WAIT_MS;
  pausedUntil.set(forge, Math.max(pausedUntil.get(forge) ?? 0, Date.now() + retryAfter));
  throttleListener(forge, retryAfter);
  return throttled(forge, url, retries - 1);
}

// Retries once after a server error or a dropped connection.
async function download(url, forge, attempts = 2) {
  let response;
  try {
    response = forge ? await throttled(forge, url) : await fetch(url);
  } catch (error) {
    if (attempts > 1) return download(url, forge, attempts - 1);
    throw error;
  }
  if (response.ok) {
    return new Uint8Array(await response.arrayBuffer());
  }
  if (attempts > 1 && response.status >= 500) {
    return download(url, forge, attempts - 1);
  }
  throw new Error(`${response.status} for ${url}`);
}
