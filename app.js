/* ETHGlobal Showcase Explorer — zero-dependency client. */
"use strict";

const $ = (id) => document.getElementById(id);
let INDEX = [];        // compact records for search/filter
let EVENTS = new Map(); // slug -> name
let state = { q: "", event: "", year: "", finalist: false };

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

async function loadJSON(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return r.json();
}

function init() {
  $("q").addEventListener("input", (e) => { state.q = e.target.value.trim().toLowerCase(); render(); });
  $("event").addEventListener("change", (e) => { state.event = e.target.value; render(); });
  $("year").addEventListener("change", (e) => { state.year = e.target.value; render(); });
  $("finalist").addEventListener("change", (e) => { state.finalist = e.target.checked; render(); });
  $("close").addEventListener("click", () => $("detail").classList.add("hidden"));
  $("detail").addEventListener("click", (e) => { if (e.target === $("detail")) $("detail").classList.add("hidden"); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") $("detail").classList.add("hidden"); });
}

function matches(rec) {
  if (state.event && rec.e !== state.event) return false;
  if (state.year && rec.d !== state.year) return false;
  if (state.finalist && !rec.f) return false;
  if (state.q) {
    const hay = rec.h; // precomputed lowercase search text
    if (!hay.includes(state.q)) {
      // word-AND fallback for long queries
      const words = state.q.split(/\s+/);
      if (!words.every((w) => hay.includes(w))) return false;
    }
  }
  return true;
}

function render() {
  const t0 = performance.now();
  const out = INDEX.filter(matches);
  const ms = (performance.now() - t0).toFixed(1);
  $("count").textContent = `${out.length.toLocaleString()} / ${INDEX.length.toLocaleString()} · ${ms} ms`;
  const frag = document.createDocumentFragment();
  if (!out.length) {
    const d = document.createElement("div");
    d.className = "empty";
    d.textContent = "No projects match.";
    frag.appendChild(d);
  }
  const max = 500; // render cap; refine search to narrow
  for (const rec of out.slice(0, max)) {
    const card = document.createElement("article");
    card.className = "card";
    card.innerHTML = `
      <h3>${esc(rec.t)}</h3>
      <div class="tag">${esc(rec.g)}</div>
      <div class="meta">
        <span class="pill">${esc(EVENTS.get(rec.e) || rec.e || "")}</span>
        ${rec.f ? '<span class="pill finalist">🏆 finalist</span>' : ""}
      </div>`;
    card.addEventListener("click", () => openDetail(rec.i));
    frag.appendChild(card);
  }
  if (out.length > max) {
    const d = document.createElement("div");
    d.className = "empty";
    d.textContent = `Showing first ${max} — refine your search.`;
    frag.appendChild(d);
  }
  const results = $("results");
  results.replaceChildren(frag);
}

async function openDetail(id) {
  let p;
  try { p = await loadJSON(`./data/p/${encodeURIComponent(id)}.json`); }
  catch { return; }
  const links = [];
  if (p.demo_url) links.push(`<a href="${esc(p.demo_url)}" target="_blank" rel="noopener">Live Demo</a>`);
  if (p.source_url) links.push(`<a href="${esc(p.source_url)}" target="_blank" rel="noopener">Source Code</a>`);
  if (p.showcase_url) links.push(`<a href="${esc(p.showcase_url)}" target="_blank" rel="noopener">ETHGlobal ↗</a>`);
  const prizes = (p.prizes || []).map((z) =>
    `<li>${esc([z.placement, z.sponsor].filter(Boolean).join(" — ") || "prize")}</li>`).join("");
  $("detail-body").innerHTML = `
    <h2>${esc(p.title)}</h2>
    <p style="color:var(--muted);margin:0.2rem 0 0">${esc(p.tagline)}</p>
    <div class="links">${links.join("")}</div>
    <div class="meta" style="display:flex;gap:0.4rem;flex-wrap:wrap">
      <span class="pill">${esc(p.event ? p.event.name : "")}</span>
      ${p.finalist ? '<span class="pill finalist">🏆 finalist</span>' : ""}
      ${p.scores ? `<span class="pill">orig ${esc(p.scores.originality ?? "–")} · prac ${esc(p.scores.practicality ?? "–")} · tech ${esc(p.scores.technicality ?? "–")}</span>` : ""}
    </div>
    ${prizes ? `<h4>Prizes</h4><ul>${prizes}</ul>` : ""}
    ${p.description ? `<h4>Description</h4><p>${esc(p.description)}</p>` : ""}
    ${p.how_its_made ? `<h4>How it's Made</h4><p>${esc(p.how_its_made)}</p>` : ""}
  `;
  $("detail").classList.remove("hidden");
}

(async function boot() {
  init();
  try {
    const [stats, events, index] = await Promise.all([
      loadJSON("./data/stats.json"),
      loadJSON("./data/events.json"),
      loadJSON("./data/search.json"),
    ]);
    INDEX = index;
    EVENTS = new Map(events.map((e) => [e.slug, e.name]));
    $("stats").textContent =
      `${Number(stats.projects).toLocaleString()} projects · ${Number(stats.events).toLocaleString()} events · ` +
      `${Number(stats.finalists).toLocaleString()} finalists · updated ${new Date(stats.generated_at).toLocaleDateString()}`;

    const years = [...new Set(INDEX.map((r) => r.d).filter(Boolean))].sort().reverse();
    for (const y of years) $("year").insertAdjacentHTML("beforeend", `<option value="${esc(y)}">${esc(y)}</option>`);
    const evs = events.filter((e) => INDEX.some((r) => r.e === e.slug));
    for (const e of evs) $("event").insertAdjacentHTML("beforeend", `<option value="${esc(e.slug)}">${esc(e.name)}</option>`);
    render();
  } catch (e) {
    $("stats").textContent = `failed to load dataset: ${e.message}`;
  }
})();
