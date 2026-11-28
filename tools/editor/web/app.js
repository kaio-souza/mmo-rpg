// Content editor page (tools/editor). No dependencies.
// Forms come from the JSON schema of each content file; the server checks
// every change with the game's own loader, so no rule of the game lives
// here. The page never leaves localhost.
"use strict";

const S = {
  ui: {}, uiLocale: "en", meta: null, content: null, schemas: {}, texts: [], art: null,
  view: "home", cat: "item", sel: null, value: null, raw: null, kind: null, isNew: false,
  extra: null, original: "", pendingTexts: {}, saveReport: null, filter: "",
  textLocale: "pt-BR", textFilter: "missing", artKind: "sprite", artFolder: null,
  checking: false, saving: false, checkTimer: 0,
  lf: { sort: "name", status: "all", facets: {}, group: "" }, lfCat: null,
  textPrefix: "", artUnused: false,
};

const $ = (id) => document.getElementById(id);
const t = (k) => S.ui[k] ?? k;
const fmt = (k, vars) => Object.entries(vars).reduce((s, [a, b]) => s.replaceAll(`{${a}}`, b), t(k));
const flabel = (k) => S.ui["editor.field." + k] ?? humanize(k);
// Choices are labelled by their name in snake case: `NotStarted` -> `editor.value.not_started`.
const vlabel = (v) => S.ui["editor.value." + String(v).replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase()] ?? String(v);

function humanize(k) {
  const s = String(k).replace(/_(permille|ticks|secs)$/, "").replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function storeGet(k) { try { return localStorage.getItem(k); } catch (_) { return null; } }
function storeSet(k, v) { try { localStorage.setItem(k, v); } catch (_) { /* private window */ } }

function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (k === "class") e.className = v;
    else if (v !== undefined && v !== null && v !== false) e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of kids.flat()) if (c !== null && c !== undefined && c !== false) e.append(c.nodeType ? c : String(c));
  return e;
}

// Line icons drawn for this page (24×24, stroke).
const ICONS = {
  sound: '<path d="M4 9h4l5-4v14l-5-4H4z"/><path d="M16 9a3 3 0 0 1 0 6"/><path d="M18.5 6.5a7 7 0 0 1 0 11"/>',
  item: '<path d="M14.5 3.5H20.5V9.5L10 20L4 14Z"/><path d="M3 21l3-3"/><path d="M7.5 13.5l3 3"/>',
  npc: '<circle cx="12" cy="8" r="4"/><path d="M4 21c0-4 3.6-6 8-6s8 2 8 6"/>',
  monster: '<path d="M5 11a7 7 0 0 1 14 0v4l-2 2v3H7v-3l-2-2z"/><circle cx="9.5" cy="12" r="1.3"/><circle cx="14.5" cy="12" r="1.3"/>',
  quest: '<path d="M6 21V4h11l-2.5 4L17 12H6"/>',
  dialogue: '<path d="M4 5h16v11H10l-5 4v-4H4z"/>',
  technique: '<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>',
  proficiency: '<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r="1"/>',
  class: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/>',
  title: '<path d="M3 8l4.5 4L12 5l4.5 7L21 8l-2 11H5z"/>',
  dungeon: '<path d="M4 21V9l8-6 8 6v12M9 21v-6h6v6"/>',
  ranking: '<path d="M4 20V10h4v10M10 20V4h4v16M16 20v-7h4v7"/>',
  special_attribute: '<path d="M12 3l2.6 5.6 6.1.7-4.5 4.2 1.2 6L12 16.6 6.6 19.5l1.2-6L3.3 9.3l6.1-.7z"/>',
  map: '<path d="M9 4 3 6v14l6-2 6 2 6-2V4l-6 2z"/><path d="M9 4v14M15 6v14"/>',
  rules: '<path d="M5 4h14v16H5z"/><path d="M9 8h6M9 12h6M9 16h3"/>',
  advanced: '<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="2"/><path d="M21 16l-5-5-9 9"/>',
  flag: '<path d="M5 21V4"/><path d="M5 4h11l-2 4 2 4H5"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.2-4.2"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7"/>',
  alert: '<path d="M12 3 2 20h20z"/><path d="M12 10v4M12 17v.5"/>',
  arrow: '<path d="M7 17 17 7M9 7h8v8"/>',
  chevron: '<path d="M6 9l6 6 6-6"/>',
  upload: '<path d="M12 16V4M7 9l5-5 5 5M4 20h16"/>',
  home: '<path d="M3 11 12 4l9 7"/><path d="M5 10v10h14V10"/><path d="M10 20v-6h4v6"/>',
  cube: '<path d="M12 2 21 7v10l-9 5-9-5V7z"/><path d="M3 7l9 5 9-5M12 12v10"/>',
  tiles: '<path d="M12 3l8 4-8 4-8-4z"/><path d="M4 12l8 4 8-4"/><path d="M4 17l8 4 8-4"/>',
  crown: '<path d="M3 8l4.5 4L12 5l4.5 7L21 8l-2 11H5z"/><path d="M5 19h14"/>',
  chart: '<path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/>',
  shield: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4"/>',
  play: '<circle cx="12" cy="12" r="9"/><path d="M10 8.5v7l6-3.5z"/>',
  palette: '<path d="M12 3a9 9 0 1 0 0 18c1.4 0 2-1 2-2s-1-1.6-1-2.6 1-1.4 2-1.4h2a4 4 0 0 0 4-4c0-4.4-4-8-9-8z"/><circle cx="7.5" cy="11" r="1.2"/><circle cx="10" cy="7" r="1.2"/><circle cx="15" cy="7.5" r="1.2"/>',
};

function icon(name, size = 18) {
  const s = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  s.setAttribute("viewBox", "0 0 24 24");
  s.setAttribute("width", size);
  s.setAttribute("height", size);
  s.setAttribute("class", "ic");
  s.innerHTML = ICONS[name] || ICONS.cube;
  return s;
}

async function api(path, body, raw) {
  const opts = body === undefined ? {} : raw
    ? { method: "POST", body }
    : { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) };
  const r = await fetch(path, opts);
  const j = await r.json();
  if (!r.ok) throw new Error(j.error || r.statusText);
  return j;
}

// Lowercase, no accents: what search and ids compare.
function norm(s) {
  return String(s ?? "").normalize("NFKD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

// An id from a name: "Espada Rúnica +2" -> "espada_runica_2". The rule of
// ids (a-z, 0-9, _) is applied here so nobody has to learn it.
function slug(s) {
  return norm(s).replace(/ß/g, "ss").replace(/æ/g, "ae").replace(/œ/g, "oe").replace(/ø/g, "o")
    .replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "").slice(0, 40).replace(/_+$/, "");
}

// ---------- toasts and dialogs ----------

function toast(msg, kind = "ok") {
  const n = el("div", { class: "toast " + kind }, icon(kind === "err" ? "alert" : "check", 16), el("span", {}, msg));
  $("toasts").append(n);
  setTimeout(() => n.classList.add("out"), 3200);
  setTimeout(() => n.remove(), 3600);
}

// Dialogs stack: a picker may open the upload dialog on top of itself.
// Resolves with the chosen button's value, or null when dismissed.
function dialog(title, bodyNodes, buttons, opts = {}) {
  return new Promise((resolve) => {
    let veil = null;
    const close = (v) => {
      document.removeEventListener("keydown", key, true);
      veil.remove();
      resolve(v);
    };
    const press = (b) => { if (b.primary && opts.validate && !opts.validate()) return; close(b.value); };
    const key = (e) => {
      if ($("modal").lastElementChild !== veil) return;
      if (e.key === "Escape") { e.preventDefault(); close(null); }
      if (e.key === "Enter" && e.target.tagName !== "TEXTAREA" && e.target.tagName !== "BUTTON") {
        const go = buttons.find((b) => b.primary);
        if (go) { e.preventDefault(); press(go); }
      }
    };
    const box = el("div", { class: "modal" + (opts.wide ? " wide" : "") },
      el("div", { class: "modal-head" }, el("h3", {}, title),
        el("button", { class: "icon-btn", onclick: () => close(null) }, icon("x"))),
      el("div", { class: "modal-body" }, ...bodyNodes),
      buttons.length ? el("div", { class: "modal-foot" },
        buttons.map((b) => el("button", { class: b.primary ? "primary" : "ghost", onclick: () => press(b) }, b.label))) : null);
    veil = el("div", { class: "veil", onmousedown: (e) => { if (e.target === veil) close(null); } }, box);
    $("modal").append(veil);
    document.addEventListener("keydown", key, true);
    setTimeout(() => (box.querySelector("[autofocus]") || box.querySelector("input, select"))?.focus());
    if (opts.onReady) opts.onReady(close);
  });
}

function say(title, text) {
  return dialog(title, [el("p", {}, text)], [{ label: t("editor.ok"), value: true, primary: true }]);
}

function askLeave() {
  return dialog(t("editor.unsaved"), [], [
    { label: t("editor.leave"), value: true },
    { label: t("editor.stay"), value: false, primary: true },
  ]);
}

// ---------- load ----------

async function boot() {
  S.meta = await api("/api/meta");
  const nav = navigator.language || "en";
  const pick = storeGet("editor-ui.locale") || S.meta.locales.find((l) => nav === l)
    || S.meta.locales.find((l) => nav.startsWith(l.split("-")[0])) || "en";
  await loadUi(pick);
  await Promise.all(S.meta.kinds.map((k) => schemaOf(k.name)));
  await Promise.all([loadTexts(), reloadContent()]);
  const cat = storeGet("editor-ui.cat");
  if (cat && cats().some((c) => c.id === cat)) S.cat = cat;
  document.addEventListener("keydown", (e) => {
    const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement?.tagName);
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      if (!$("modal").childElementCount) quickOpen();
    } else if (e.key === "/" && !typing && !$("modal").childElementCount) {
      const box = document.querySelector("#side input[type=search]");
      if (box) { e.preventDefault(); box.focus(); box.select(); }
    }
    const z = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && (z === "z" || z === "y") && S.view === "content" && S.sel && S.kind !== "map"
      && !typing && !$("modal").childElementCount) {
      e.preventDefault();
      stepUndo(z === "z" && !e.shiftKey);
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (S.view === "content" && S.sel) submit(true);
      else if (S.view === "looks" && looksDirty()) saveTiles();
      else if (S.view === "langs" && langsDirty()) saveLangs();
      else if (S.view === "sounds" && soundsDirty()) saveSounds();
      else if (S.view === "game" && gameDirty()) saveGame();
      else if (S.view === "theme" && themeDirty()) saveTheme();
    }
  });
  syncKeys();
  draw();
}

async function loadUi(locale) {
  S.ui = await api("/api/ui/" + locale);
  S.uiLocale = locale;
  document.documentElement.lang = locale;
  document.title = t("editor.title");
}

async function reloadContent() {
  S.content = await api("/api/content");
  S.report = S.content.report;
  S.links = null;
}

async function loadTexts() { S.texts = await api("/api/texts"); }
async function loadArt() { S.art = await api("/api/art"); }

async function schemaOf(kind) {
  if (!S.schemas[kind]) S.schemas[kind] = await api("/api/schema/" + kind);
  return S.schemas[kind];
}

// ---------- categories ----------

// What the left rail shows. Every kind the server knows and no category
// names lands in "advanced", so a new content type is never hidden.
const CATS = [
  { id: "item", kinds: ["item"] },
  { id: "npc", kinds: ["npc"] },
  { id: "monster", kinds: ["monster"] },
  { id: "quest", kinds: ["quest"] },
  { id: "dialogue", kinds: ["dialogue"] },
  { id: "technique", kinds: ["technique"] },
  { id: "proficiency", kinds: ["proficiency"] },
  { id: "class", kinds: ["class"] },
  { id: "title", kinds: ["title"] },
  { id: "special_attribute", kinds: ["special_attribute"] },
  { id: "ranking", kinds: ["ranking"] },
  { id: "dungeon", kinds: ["dungeon"] },
  { id: "goal", kinds: ["goal"] },
  { id: "map", kinds: ["map"] },
  { id: "shop", kinds: ["shop"] },
  { id: "recipe", kinds: ["recipe"] },
  { id: "gather_node", kinds: ["gather_node"] },
  { id: "emote", kinds: ["emote"] },
  { id: "loot_table", kinds: ["loot_table"] },
  { id: "rules", kinds: ["progression_formulas", "combat_formulas", "economy_formulas", "counters"] },
  { id: "advanced", kinds: ["appearance", "moderation_reasons", "raw"] },
];

// The files of a zone besides its map: the map page edits them together,
// with the seam band and the passages in sight, so they are not listed.
const ZONE_FILES = ["zone", "zone_npcs", "zone_monsters", "zone_nodes"];

function cats() {
  const known = new Set([...CATS.flatMap((c) => c.kinds), ...ZONE_FILES]);
  const rest = S.meta.kinds.map((k) => k.name).filter((n) => !known.has(n));
  return CATS.filter((c) => c.id === "advanced" || kindInfo(c.kinds[0]))
    .map((c) => (c.id === "advanced" ? { ...c, kinds: [...c.kinds.slice(0, -1), ...rest, "raw"] } : c));
}

function curCat() { return cats().find((c) => c.id === S.cat) || cats()[0]; }
function catOf(kind) { return cats().find((c) => c.kinds.includes(kind ?? "raw")) || curCat(); }
function catLabel(c) {
  if (c.id === "advanced") return t("editor.cat.advanced");
  if (c.id === "rules") return t("editor.cat.rules");
  return t("editor.kind." + c.id);
}

function filesOfKind(k) {
  return S.content.files.filter((f) => !f.path.startsWith("text/") && (f.kind ?? "raw") === k);
}
function filesOf(c) { return c.kinds.flatMap(filesOfKind); }

function kindInfo(name) { return S.meta.kinds.find((k) => k.name === name); }

// The single kind a category can create, when there is one.
function creatable(c) {
  const k = c.kinds.length === 1 ? kindInfo(c.kinds[0]) : null;
  return k && k.per_id ? k.name : null;
}

// ---------- entries ----------

function rowOf(key) { return S.texts.find((r) => r.key === key); }
function textOf(key) { return S.pendingTexts[key]?.text ?? rowOf(key)?.texts.en ?? ""; }

function nameKey(kind, id) {
  const k = kindInfo(kind);
  const tpl = k && id ? k.texts.find((x) => x.endsWith(".name")) : null;
  return tpl ? tpl.replace("{id}", id) : null;
}

function titleOf(kind, id) {
  const nk = nameKey(kind, id);
  return (nk && textOf(nk)) || id;
}

function refTitle(kind, id) {
  const n = titleOf(kind, id);
  return n && n !== id ? `${n} (${id})` : id;
}

function fileState(path, kind, id) {
  // A map stands for its whole zone: the zone's problems mark it too.
  const zone = (kind === "map" || kind?.startsWith("zone")) && new RegExp(`zones/${id}[/:]`);
  const errs = S.report.errors.filter((e) => e.includes(path) || (zone && zone.test(e)));
  if (errs.length) return "err";
  const k = kindInfo(kind);
  if (k && id && k.texts.some((x) => S.report.missing[x.replace("{id}", id)])) return "miss";
  return "ok";
}

function picOf(b) {
  if (!b) return null;
  if (b.icon) return ["icon", b.icon];
  if (b.sprite) return ["sprite", b.sprite];
  return null;
}

// Tags under a name: the plain choices that say what it is (an enum that
// has no default, or that differs from its default) and the level.
function briefTags(kind, b) {
  const s = S.schemas[kind];
  if (!s?.properties || !b) return [];
  const out = [];
  for (const [k, ps] of Object.entries(s.properties)) {
    const v = b[k];
    if (v === undefined) continue;
    if (shape(s, ps) === "enum" && typeof v === "string") {
      const d = R(s, ps).default ?? ps.default;
      if (d === undefined || d !== v) out.push(vlabel(v));
    } else if ((k === "level" || k === "tier" || k === "unlock_at") && typeof v === "number") {
      out.push(`${flabel(k)} ${v}`);
    } else if (typeof v === "string" && k !== "icon" && k !== "sprite" && kindInfo(k)) {
      out.unshift(titleOf(k, v));
    }
  }
  return out.slice(0, 3);
}

function entryOf(f) {
  const kind = f.kind ?? "raw";
  // A file that is the only one of its kind (the formulas, counters.ron)
  // is known by its kind's name, with the path under it.
  const single = !f.id && kind !== "raw" && S.ui["editor.kind." + kind];
  const title = single || (f.id && titleOf(kind, f.id)) || f.id || f.path;
  const tags = briefTags(kind, f.brief);
  let sub = tags.length ? tags.join(" · ") : title !== f.id && f.id ? f.id : single ? f.path : "";
  let pic = picOf(f.brief);
  // A dialogue has no name of its own: it is known by who speaks it.
  if (kind === "dialogue") {
    const npcs = S.content.files.filter((n) => n.kind === "npc" && n.brief?.dialogue === f.id);
    if (npcs.length) { sub = npcs.map((n) => titleOf("npc", n.id)).join(", "); pic = picOf(npcs[0].brief); }
    else sub = t("editor.links.nobody");
  }
  return { path: f.path, kind, id: f.id, title, sub, pic, brief: f.brief || {}, state: fileState(f.path, kind, f.id) };
}

const artUrl = (kind, name) => `/api/art/${kind}/${encodeURIComponent(name)}`;

function hue(s) {
  let h = 0;
  for (const c of String(s)) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}

function thumb(title, pic, size, kind) {
  const box = el("div", { class: "thumb", style: `--s:${size}px;--h:${hue(kind || title)}` });
  if (pic) {
    const img = el("img", { src: artUrl(pic[0], pic[1]), alt: "", loading: "lazy" });
    img.addEventListener("error", () => { img.remove(); box.classList.add("letter"); box.append(letter(title)); });
    box.append(img);
  } else {
    box.classList.add("letter");
    box.append(letter(title));
  }
  return box;
}
function letter(title) { return (Array.from(String(title || "?"))[0] || "?").toUpperCase(); }

// ---------- frame ----------

// The map page asks for the whole width (map.js: mapFocus).
function syncFocus() {
  const on = S.view === "content" && S.kind === "map" && !!S.sel && typeof mapFocus === "function" && mapFocus();
  $("app").classList.toggle("focus", on);
}

function draw() {
  syncFocus();
  drawRail();
  if (S.view === "texts") { drawTextSide(); drawTexts(); }
  else if (S.view === "art") { drawArtSide(); drawArt(); }
  else if (S.view === "looks") { drawLooksSide(); drawLooks(); }
  else if (S.view === "langs") { drawLangsSide(); drawLangs(); }
  else if (S.view === "sounds") { drawSoundsSide(); drawSounds(); }
  else if (S.view === "game") { drawGameSide(); drawGame(); }
  else if (S.view === "theme") { drawThemeSide(); drawTheme(); }
  else if (S.view === "economy") { drawEconomySide(); drawEconomy(); }
  else if (S.view === "moderation") { drawModerationSide(); drawModeration(); }
  else if (S.view === "replays") { drawReplaysSide(); drawReplays(); }
  else if (S.view === "home") { drawHomeSide(); drawHome(); }
  else { drawSide(); drawPanel(); }
}

function railBtn(ic, label, on, onclick, badge) {
  return el("button", { class: "rail-btn" + (on ? " on" : ""), onclick, title: label },
    icon(ic, 18), el("span", { class: "rail-label" }, label), badge || null);
}

// The rail groups the categories by what a designer is working on.
const SECTIONS = [
  ["world", ["map", "npc", "monster", "gather_node", "shop", "dungeon"]],
  ["items", ["item", "recipe", "loot_table"]],
  ["progress", ["technique", "proficiency", "class", "title", "special_attribute", "ranking"]],
  ["story", ["quest", "dialogue", "emote", "goal"]],
  ["system", ["rules", "advanced"]],
];

function drawRail() {
  const r = $("rail");
  r.replaceChildren();
  r.append(el("div", { class: "brand" }, el("div", { class: "logo" }, icon("cube", 18)), el("span", {}, t("editor.title"))));
  r.append(el("button", { class: "quick", onclick: quickOpen, title: "Ctrl+K" },
    icon("search", 15), el("span", {}, t("editor.quick_open")), el("kbd", {}, "Ctrl K")));
  r.append(el("div", { class: "launch" },
    el("button", { class: "primary", onclick: playGame, disabled: S.playing, title: t("editor.launch.play_hint") },
      icon("play", 15), el("span", {}, t(S.playing ? "editor.launch.playing" : "editor.launch.play"))),
    el("button", { class: "ghost", onclick: newGame, title: t("editor.launch.new_game") }, icon("plus", 15),
      el("span", {}, t("editor.launch.new_game")))));
  const nav = el("nav", { class: "rail-nav" });
  nav.append(railBtn("home", t("editor.home.title"), S.view === "home", () => go("home"),
    S.report.errors.length ? el("span", { class: "badge err" }, S.report.errors.length) : null));
  const all = cats();
  for (const [sec, ids] of SECTIONS) {
    const here = all.filter((c) => ids.includes(c.id));
    if (!here.length) continue;
    // A section folds away with a click on its name; the choice is kept.
    const folded = storeGet("editor-ui.fold." + sec) === "1";
    const holds = here.some((c) => S.view === "content" && S.cat === c.id);
    nav.append(el("button", { class: "rail-group fold" + (folded ? " folded" : ""), title: t("editor.fold_hint"),
      onclick: () => { storeSet("editor-ui.fold." + sec, folded ? "0" : "1"); drawRail(); } },
    el("span", { class: "chev" }, folded ? "▸" : "▾"), t("editor.section." + sec)));
    for (const c of here) {
      // Folded, only the open category stays in sight.
      if (folded && !(holds && S.cat === c.id)) continue;
      const files = filesOf(c);
      const errs = files.filter((f) => fileState(f.path, f.kind, f.id) === "err").length;
      nav.append(railBtn(c.id, catLabel(c), S.view === "content" && S.cat === c.id, () => goCat(c.id),
        errs ? el("span", { class: "badge err" }, errs) : el("span", { class: "n" }, files.length)));
    }
  }
  const missing = Object.keys(S.report.missing).length;
  const tools = el("nav", { class: "rail-nav" },
    railBtn("image", t("editor.art"), S.view === "art", () => go("art")),
    railBtn("tiles", t("editor.looks"), S.view === "looks", () => go("looks")),
    railBtn("sound", t("editor.sounds"), S.view === "sounds", () => go("sounds")),
    railBtn("globe", t("editor.texts"), S.view === "texts", () => go("texts"),
      missing ? el("span", { class: "badge warn" }, missing) : null),
    railBtn("flag", t("editor.languages"), S.view === "langs", () => go("langs")),
    railBtn("crown", t("editor.game"), S.view === "game", () => go("game")),
    railBtn("palette", t("editor.theme"), S.view === "theme", () => go("theme")));
  // The live game: its database and its recorded zones.
  const live = el("nav", { class: "rail-nav" },
    el("div", { class: "rail-group" }, t("editor.section.live")),
    railBtn("chart", t("editor.economy"), S.view === "economy", () => go("economy")),
    railBtn("shield", t("editor.moderation"), S.view === "moderation", () => go("moderation")),
    railBtn("play", t("editor.replays"), S.view === "replays", () => go("replays")));
  const sel = el("select", { onchange: async (e) => { storeSet("editor-ui.locale", e.target.value); await loadUi(e.target.value); draw(); } },
    (S.meta.page_locales || S.meta.locales).map((l) => el("option", { value: l, selected: l === S.uiLocale }, langName(l, l))));
  r.append(nav, el("div", { class: "rail-sep" }), tools, el("div", { class: "rail-sep" }), live, el("div", { class: "grow" }),
    el("label", { class: "rail-locale", title: t("editor.locale") }, icon("globe", 15), sel));
}

function langName(code, inLocale) {
  try { return new Intl.DisplayNames([inLocale || S.uiLocale], { type: "language" }).of(code) || code; }
  catch (_) { return code; }
}

async function goCat(id) {
  if (S.view === "content" && S.cat === id) return;
  if (dirty() && !(await askLeave())) return;
  S.view = "content"; S.cat = id; S.filter = ""; S.sel = null; S.isNew = false; S.pendingTexts = {};
  storeSet("editor-ui.cat", id);
  draw();
}

async function go(view) {
  if (S.view === view) return;
  if (dirty() && !(await askLeave())) return;
  S.view = view; S.filter = ""; S.sel = null; S.isNew = false; S.pendingTexts = {};
  if (view === "texts") await loadTexts();
  if (view === "art") await loadArt();
  if (view === "looks") await loadLooks();
  if (view === "langs") await loadLangs();
  if (view === "sounds") await loadSounds();
  if (view === "game") await loadGame();
  if (view === "theme") await loadTheme();
  if (view === "economy" || view === "moderation") await loadAdmin(view);
  if (view === "replays") await loadReplays();
  if (view !== "replays" && typeof stopPlay === "function") stopPlay();
  draw();
}

function searchBox(value, oninput) {
  return el("label", { class: "search" }, icon("search", 16),
    el("input", { type: "search", placeholder: t("editor.search"), value, oninput: (e) => oninput(e.target.value) }));
}

// ---------- content list ----------

// ---------- list filters ----------

function readLf(cat) {
  const blank = { sort: "name", status: "all", facets: {}, group: "" };
  try { return { ...blank, ...JSON.parse(storeGet("editor-ui.list." + cat) || "{}") }; } catch (_) { return blank; }
}
function saveLf() { storeSet("editor-ui.list." + S.lfCat, JSON.stringify(S.lf)); }

function listEntries(c) { return c.kinds.flatMap((k) => filesOfKind(k).map(entryOf)); }

// What the list can be narrowed by: every plain choice (an enum, a
// reference such as the proficiency of a technique) that takes between
// two and sixteen different values in this category.
function facetsOf(entries) {
  const vals = {};
  for (const e of entries) {
    for (const [k, v] of Object.entries(e.brief)) {
      if (typeof v !== "string" || k === "icon" || k === "sprite") continue;
      const m = (vals[k] ??= new Map());
      m.set(v, (m.get(v) || 0) + 1);
    }
  }
  return Object.entries(vals).filter(([, m]) => m.size >= 2 && m.size <= 16)
    .map(([key, m]) => ({ key, values: [...m.keys()].sort((a, b) => facetLabel(key, a).localeCompare(facetLabel(key, b))) }));
}

function facetLabel(key, v) { return kindInfo(key) ? titleOf(key, v) : vlabel(v); }

// Numbers worth sorting by (level, tier, unlock point, value...).
function sortKeysOf(entries) {
  const seen = new Map();
  for (const e of entries) for (const [k, v] of Object.entries(e.brief)) if (typeof v === "number") seen.set(k, (seen.get(k) || 0) + 1);
  const first = ["level", "tier", "unlock_at", "value"];
  return [...seen.entries()].filter(([, n]) => n >= 2).map(([k]) => k)
    .sort((a, b) => (first.indexOf(a) + 99) % 99 - (first.indexOf(b) + 99) % 99).slice(0, 4);
}

function lfActive() {
  return S.lf.status !== "all" || Object.values(S.lf.facets).some(Boolean);
}

function sideSelect(value, options, onchange, title) {
  return el("select", { class: "sm", title, onchange: (e) => onchange(e.target.value) },
    options.map(([v, label]) => el("option", { value: v, selected: v === value }, label)));
}

function listControls(c, entries) {
  const box = el("div", { class: "lf" });
  const facets = facetsOf(entries);
  const sorts = sortKeysOf(entries);
  const redo = () => { saveLf(); drawSideList(); };
  const rows = [];
  const sortOpts = [["name", t("editor.sort.name")]];
  for (const k of sorts) sortOpts.push([k + ":asc", `${flabel(k)} ↑`], [k + ":desc", `${flabel(k)} ↓`]);
  sortOpts.push(["problems", t("editor.sort.problems")]);
  if (!sortOpts.some(([v]) => v === S.lf.sort)) S.lf.sort = "name";
  rows.push(sideSelect(S.lf.sort, sortOpts, (v) => { S.lf.sort = v; redo(); }, t("editor.sort")));
  const pics = entries.some((e) => e.pic) ? [["noimg", t("editor.status.no_image")]] : [];
  rows.push(sideSelect(S.lf.status, [["all", t("editor.filter_all")], ["err", t("editor.status.problems")], ["miss", t("editor.status.missing")], ...pics],
    (v) => { S.lf.status = v; redo(); }, t("editor.status")));
  // Sort and state are always in sight; the rest folds under "More
  // filters", open by itself while one of them is in use.
  const more = [];
  for (const f of facets) {
    if (S.lf.facets[f.key] && !f.values.includes(S.lf.facets[f.key])) delete S.lf.facets[f.key];
    more.push(sideSelect(S.lf.facets[f.key] || "",
      [["", fmt("editor.facet_all", { name: flabel(f.key) })], ...f.values.map((v) => [v, facetLabel(f.key, v)])],
      (v) => { S.lf.facets[f.key] = v; redo(); }, flabel(f.key)));
  }
  if (c.kinds.length === 1 && facets.length) {
    if (S.lf.group && !facets.some((f) => f.key === S.lf.group)) S.lf.group = "";
    more.push(sideSelect(S.lf.group, [["", t("editor.group_none")], ...facets.map((f) => [f.key, `${t("editor.group")}: ${flabel(f.key)}`])],
      (v) => { S.lf.group = v; redo(); }, t("editor.group")));
  }
  const inUse = Object.values(S.lf.facets).filter(Boolean).length + (S.lf.group ? 1 : 0);
  const open = inUse > 0 || storeGet("editor-ui.lf-more") === "1";
  box.append(el("div", { class: "lf-row" }, rows));
  if (more.length) {
    box.append(el("button", { class: "link lf-more", onclick: () => {
      storeSet("editor-ui.lf-more", open ? "0" : "1"); drawSide();
    } }, (open ? "▾ " : "▸ ") + (inUse ? fmt("editor.more_filters_n", { n: inUse }) : t("editor.more_filters"))));
    if (open) box.append(el("div", { class: "lf-row" }, more));
  }
  box.append(el("div", { class: "lf-count" }, el("span", { id: "lf-n" }),
    el("button", { class: "link", id: "lf-clear", hidden: true, onclick: () => {
      S.filter = ""; S.lf.status = "all"; S.lf.facets = {}; saveLf(); drawSide();
    } }, t("editor.clear_filters"))));
  return box;
}

// The search field is built once and the list under it is what gets
// redrawn, so typing never loses the caret.
function drawSide() {
  const side = $("side");
  side.replaceChildren();
  const c = curCat();
  if (S.lfCat !== c.id) { S.lf = readLf(c.id); S.lfCat = c.id; }
  const make = creatable(c);
  side.append(el("div", { class: "side-head" },
    el("h2", {}, catLabel(c)), el("span", { class: "count" }, filesOf(c).length), el("div", { class: "grow" }),
    make ? el("button", { class: "primary sm", onclick: () => startNew(make) }, icon("plus", 16), t("editor.new")) : null));
  const search = searchBox(S.filter, (v) => { S.filter = v; drawSideList(); });
  const input = search.querySelector("input");
  input.placeholder = t("editor.search") + "  ( / )";
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); $("sidelist")?.querySelector(".entry")?.focus(); }
    else if (e.key === "Escape" && input.value) { e.preventDefault(); input.value = ""; S.filter = ""; drawSideList(); }
  });
  side.append(search);
  side.append(listControls(c, listEntries(c)));
  const list = el("div", { id: "sidelist", class: "list" });
  list.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const all = [...list.querySelectorAll(".entry")];
    const i = all.indexOf(document.activeElement);
    if (i < 0) return;
    e.preventDefault();
    if (e.key === "ArrowUp" && i === 0) input.focus();
    else all[Math.max(0, Math.min(all.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
  });
  side.append(list);
  drawSideList();
}

function entryRow(e) {
  return el("button", { class: "entry" + (S.sel === e.path ? " sel" : ""), onclick: () => open(e.path) },
    thumb(e.title, e.pic, 36, e.kind),
    el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, e.title),
      e.sub ? el("div", { class: "entry-sub" }, e.sub) : null),
    e.state !== "ok" ? el("span", { class: "dot " + e.state,
      title: e.state === "err" ? t("editor.errors") : t("editor.missing_translation") }) : null);
}

function drawSideList() {
  const box = $("sidelist");
  if (!box) return;
  const keep = box.scrollTop;
  box.replaceChildren();
  const c = curCat();
  const q = norm(S.filter);
  const lf = S.lf;
  const rank = { err: 0, miss: 1, ok: 2 };
  const by = (a, b) => a.title.localeCompare(b.title);
  let cmp = by;
  if (lf.sort === "problems") cmp = (a, b) => rank[a.state] - rank[b.state] || by(a, b);
  else if (lf.sort.includes(":")) {
    const [k, dir] = lf.sort.split(":");
    const sign = dir === "desc" ? -1 : 1;
    cmp = (a, b) => sign * ((a.brief[k] ?? Infinity) - (b.brief[k] ?? Infinity)) || by(a, b);
  }
  let shown = 0, total = 0;
  for (const k of c.kinds) {
    const mine = filesOfKind(k).map(entryOf);
    total += mine.length;
    const entries = mine
      .filter((e) => !q || norm(e.title).includes(q) || norm(e.id || e.path).includes(q) || norm(e.sub).includes(q))
      .filter((e) => lf.status === "all" || (lf.status === "noimg" ? !e.pic : e.state === lf.status))
      .filter((e) => Object.entries(lf.facets).every(([f, v]) => !v || e.brief[f] === v))
      .sort(cmp);
    if (!entries.length) continue;
    if (c.kinds.length > 1) box.append(el("div", { class: "list-group" }, t("editor.kind." + k)));
    if (lf.group && c.kinds.length === 1) {
      const groups = new Map();
      for (const e of entries) {
        const g = e.brief[lf.group] ?? "";
        if (!groups.has(g)) groups.set(g, []);
        groups.get(g).push(e);
      }
      const order = [...groups.keys()].sort((a, b) => facetLabel(lf.group, a).localeCompare(facetLabel(lf.group, b)));
      for (const g of order) {
        box.append(el("div", { class: "list-group" }, `${g ? facetLabel(lf.group, g) : "—"} · ${groups.get(g).length}`));
        for (const e of groups.get(g)) box.append(entryRow(e));
      }
    } else {
      for (const e of entries) box.append(entryRow(e));
    }
    shown += entries.length;
  }
  if (!shown) box.append(el("div", { class: "empty-list" }, S.filter || lfActive() ? t("editor.no_results") : t("editor.empty_list")));
  const n = $("lf-n");
  if (n) n.textContent = fmt("editor.shown", { n: shown, m: total });
  const clear = $("lf-clear");
  if (clear) clear.hidden = !(S.filter || lfActive());
  box.scrollTop = keep;
}

// ---------- quick open ----------

// Ctrl+K: find anything by name or id, whatever its category.
async function quickOpen() {
  const all = S.content.files.filter((f) => f.kind && f.id).map(entryOf);
  const input = el("input", { class: "big", autofocus: true, placeholder: t("editor.quick_placeholder") });
  const list = el("div", { class: "qo-list" });
  let hits = [], cur = 0, close = () => {};
  const render = () => {
    list.replaceChildren();
    hits.forEach((e, i) => list.append(el("button", { class: "entry" + (i === cur ? " sel" : ""), onclick: () => close(e.path) },
      thumb(e.title, e.pic, 32, e.kind),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, e.title), el("div", { class: "entry-sub" }, e.id)),
      el("span", { class: "chip" }, t("editor.kind." + e.kind)))));
    if (!hits.length) list.append(el("div", { class: "empty-list" }, t("editor.quick_none")));
    list.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
  };
  const search = () => {
    const words = norm(input.value).split(/\s+/).filter(Boolean);
    const score = (e) => {
      const title = norm(e.title), id = norm(e.id), kind = norm(t("editor.kind." + e.kind));
      if (!words.every((w) => title.includes(w) || id.includes(w) || kind.includes(w))) return -1;
      return words.reduce((s, w) => s + (title.startsWith(w) ? 0 : title.includes(w) ? 1 : id.includes(w) ? 2 : 3), 0);
    };
    hits = all.map((e) => [score(e), e]).filter(([sc]) => sc >= 0)
      .sort((a, b) => a[0] - b[0] || a[1].title.localeCompare(b[1].title)).slice(0, 40).map(([, e]) => e);
    cur = 0;
    render();
  };
  input.addEventListener("input", search);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); cur = Math.min(hits.length - 1, cur + 1); render(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); cur = Math.max(0, cur - 1); render(); }
    else if (e.key === "Enter" && hits[cur]) { e.preventDefault(); close(hits[cur].path); }
  });
  search();
  const path = await dialog(t("editor.quick_open"), [input, list], [], { onReady: (c) => { close = c; }, wide: true });
  if (path) await open(path);
}

async function open(path) {
  // A zone file is edited on its map.
  const zf = /^zones\/([^/]+)\/(zone|npcs|monsters|nodes)\.ron$/.exec(path || "");
  if (zf) path = `zones/${zf[1]}/map.ron`;
  if (path === S.sel) return;
  if (dirty() && !(await askLeave())) return;
  const f = await api("/api/file?path=" + encodeURIComponent(path));
  rememberRecent(path);
  S.sel = path; S.kind = f.kind; S.isNew = false; S.pendingTexts = {}; S.saveReport = null;
  S.value = f.json; S.raw = f.json ? null : f.text; S.extra = null;
  if (f.kind) await schemaOf(f.kind);
  if (f.kind === "map") await mapLoad(path);
  S.original = snap();
  resetUndo();
  const c = catOf(f.kind);
  if (S.view !== "content" || c.id !== S.cat) { S.view = "content"; S.cat = c.id; S.filter = ""; drawRail(); drawSide(); }
  else drawSideList();
  drawPanel();
  $("panel").scrollTop = 0;
}

// What a save would change: the file, and the zone files of a map.
function snap() { return JSON.stringify([S.value, S.raw, S.extra ?? null]); }
function dirty() {
  if (S.view === "looks") return looksDirty();
  if (S.view === "langs") return langsDirty();
  if (S.view === "sounds") return soundsDirty();
  if (S.view === "game") return gameDirty();
  if (S.view === "theme") return themeDirty();
  return !!S.sel && (S.isNew || snap() !== S.original || Object.keys(S.pendingTexts).length > 0);
}

// ---------- creating ----------

// A new piece of content starts from its name. The id is made from the
// name and only shown; it can still be changed, and it is made unique.
// Copying an existing one gives a file that already passes the rules; an
// empty one only has the fields the format requires, and the live check
// says what is missing.
async function startNew(kind, fromId) {
  if (dirty() && !(await askLeave())) return;
  const info = kindInfo(kind);
  const taken = new Set(S.content.ids[kind] || []);
  const unique = (base) => {
    if (!taken.has(base)) return base;
    let n = 2;
    while (taken.has(`${base}_${n}`)) n++;
    return `${base}_${n}`;
  };
  let manual = false;
  const name = el("input", { class: "big", autofocus: true, placeholder: t("editor.name") });
  const idCode = el("code", { class: "id-code" });
  const idInput = el("input", { class: "mono id-input", hidden: true });
  const idErr = el("span", { class: "err-text" });
  const currentId = () => manual ? slug(idInput.value) : unique(slug(name.value) || `${kind}_${taken.size + 1}`);
  const refresh = () => {
    const id = currentId();
    idCode.textContent = id || "—";
    const clash = !!id && taken.has(id);
    idErr.textContent = clash ? t("editor.id_taken") : "";
    idInput.classList.toggle("bad", clash || !id);
    return !!id && !clash && (manual || name.value.trim() !== "");
  };
  const edit = el("button", { class: "link", onclick: () => {
    idInput.value = currentId(); manual = true;
    idCode.hidden = true; edit.hidden = true; idInput.hidden = false; idInput.focus();
  } }, t("editor.edit_id"));
  name.addEventListener("input", refresh);
  idInput.addEventListener("input", refresh);
  const examples = [...taken].map((id) => ({ id, title: refTitle(kind, id) })).sort((a, b) => a.title.localeCompare(b.title));
  const from = el("select", {},
    el("option", { value: "" }, t("editor.new_empty")),
    examples.map((e) => el("option", { value: e.id }, `${t("editor.new_copy")} ${e.title}`)));
  // A zone starts empty (a copy of a whole zone is rarely wanted); the
  // rest start from a copy, which already passes the rules.
  if (examples.length && kind !== "map") from.value = examples[0].id;
  if (fromId) from.value = fromId;
  if (kind === "map" && !M.tiles) M.tiles = await api("/api/tiles");
  const mapOpts = kind === "map" ? mapNewOptions() : null;
  if (mapOpts) {
    from.addEventListener("change", () => { mapOpts.box.hidden = !!from.value; });
    mapOpts.box.hidden = !!from.value;
  }
  refresh();
  const body = [
    el("div", { class: "fld" }, el("label", {}, t("editor.name")), name,
      info.texts.length ? el("div", { class: "hint" }, t("editor.name_hint")) : null,
      el("div", { class: "id-line" }, el("span", { class: "hint" }, t("editor.id")), idCode, idInput, edit, idErr)),
    el("div", { class: "fld" }, el("label", {}, t("editor.new_from")), from),
    mapOpts?.box,
  ];
  const ok = await dialog(`${t("editor.new")} · ${t("editor.kind." + kind)}`, body, [
    { label: t("editor.cancel"), value: false },
    { label: t("editor.create"), value: true, primary: true },
  ], { validate: refresh });
  if (!ok) return;
  const id = currentId();
  const path = info.path.replace("{id}", id);

  S.sel = path; S.kind = kind; S.isNew = true; S.pendingTexts = {}; S.saveReport = null; S.extra = null;
  if (from.value) {
    const src = await api("/api/file?path=" + encodeURIComponent(info.path.replace("{id}", from.value)));
    S.value = src.json ?? null;
    S.raw = src.json ? null : src.text;
  } else {
    const s = await schemaOf(kind);
    S.value = defaultOf(s, s);
    S.raw = null;
  }
  // A map never stands alone: creating one creates the whole zone.
  if (kind === "map") await mapNew(from.value, mapOpts.get());
  const nk = nameKey(kind, id);
  if (nk && name.value.trim()) S.pendingTexts[nk] = { text: name.value.trim() };
  S.original = "";
  resetUndo();
  S.view = "content"; S.cat = catOf(kind).id; S.filter = "";
  draw();
  $("panel").scrollTop = 0;
  // Say right away what a brand-new file still needs.
  submit(false);
}

// The id inside a path, read from the kind's own `{id}` template — the
// same way the server reads it. A file that is not saved yet is not in
// the file list, so its id has to come from here.
function idOfPath(kind, path) {
  const info = kindInfo(kind);
  if (!info || !info.per_id) return "";
  const [pre, post] = info.path.split("{id}");
  if (!path.startsWith(pre) || !path.endsWith(post)) return "";
  const id = path.slice(pre.length, path.length - post.length);
  return id && !id.includes("/") ? id : "";
}

// ---------- the open file ----------

// The game's home, the first page: every problem of the content, the
// texts still missing, every category at a glance, and what was opened last.
function drawHomeSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.home.title"))),
    el("p", { class: "hint side-hint" }, t("editor.home.hint")));
}

function drawHome() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page home" });
  const errs = S.report.errors, warns = S.report.warnings || [];
  const missing = Object.keys(S.report.missing).length;
  // What blocks the game first, then what only needs a look.
  const probs = el("section", { class: "card home-card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.home.problems")), el("span", { class: "n" }, errs.length + warns.length)));
  if (!errs.length && !warns.length) probs.append(el("p", { class: "home-ok" }, icon("check", 16), t("editor.home.no_problems")));
  for (const [list, cls] of [[errs, "err"], [warns, "warn"]]) {
    if (list.length) probs.append(el("ul", { class: "home-probs " + cls },
      list.slice(0, 30).map((m) => el("li", {}, clip(m), fileLink(m)))));
  }
  page.append(el("div", { class: "home-head" },
    el("div", { class: "empty-ic" }, icon("home", 32)),
    el("div", {}, el("h2", {}, t("editor.home.title")), el("p", { class: "hint" }, t("editor.home.hint")))), probs);
  if (missing) {
    page.append(el("section", { class: "card home-card" },
      el("div", { class: "card-head" }, el("h3", {}, t("editor.home.texts")), el("span", { class: "n" }, missing)),
      el("p", { class: "hint" }, fmtN("editor.home.texts_missing", missing)),
      el("button", { onclick: () => go("texts") }, icon("globe", 15), t("editor.texts"))));
  }
  // Every category: how many, and how many with a problem.
  const grid = el("div", { class: "home-grid" });
  for (const [sec, ids] of SECTIONS) {
    for (const c of cats().filter((x) => ids.includes(x.id))) {
      const files = filesOf(c);
      const bad = files.filter((f) => fileState(f.path, f.kind, f.id) === "err").length;
      grid.append(el("button", { class: "home-cat", onclick: () => goCat(c.id), title: t("editor.section." + sec) },
        icon(c.id, 20), el("span", { class: "home-t" }, catLabel(c)),
        bad ? el("span", { class: "badge err" }, bad) : el("span", { class: "n" }, files.length)));
    }
  }
  page.append(el("section", { class: "card home-card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.home.content"))), grid));
  const byPath = new Map(S.content.files.map((f) => [f.path, f]));
  const recent = recentPaths().map((x) => byPath.get(x)).filter(Boolean).map(entryOf).slice(0, 10);
  if (recent.length) {
    page.append(el("section", { class: "card home-card" },
      el("div", { class: "card-head" }, el("h3", {}, t("editor.home.recent"))),
      el("div", { class: "home-list" }, recent.map((e) => el("button", { class: "home-row", onclick: () => open(e.path) },
        thumb(e.title, e.pic, 28, e.kind), el("span", { class: "home-t" }, e.title),
        el("span", { class: "chip" }, t("editor.kind." + e.kind)))))));
  }
  p.append(page);
}

// The files opened last, newest first (this browser only).
function recentPaths() {
  try { return JSON.parse(storeGet("editor-ui.recent") || "[]"); } catch (_) { return []; }
}
function rememberRecent(path) {
  const list = [path, ...recentPaths().filter((p) => p !== path)].slice(0, 30);
  storeSet("editor-ui.recent", JSON.stringify(list));
}

// With nothing open, the category's home: what needs a look (problems,
// missing translations), what was opened last, and Create.
function emptyState() {
  const c = curCat();
  const make = creatable(c);
  const entries = listEntries(c);
  const byPath = new Map(entries.map((e) => [e.path, e]));
  const card = (title, list) => el("section", { class: "card home-card" },
    el("div", { class: "card-head" }, el("h3", {}, title), el("span", { class: "n" }, list.length)),
    el("div", { class: "home-list" }, list.slice(0, 8).map((e) => el("button", { class: "home-row", onclick: () => open(e.path) },
      thumb(e.title, e.pic, 28, e.kind), el("span", { class: "home-t" }, e.title),
      e.state !== "ok" ? el("span", { class: "badge " + (e.state === "err" ? "err" : "warn") },
        t(e.state === "err" ? "editor.home.has_problem" : "editor.home.untranslated")) : null))));
  const needs = entries.filter((e) => e.state !== "ok").sort((a, b) => (a.state === "err" ? 0 : 1) - (b.state === "err" ? 0 : 1));
  const recent = recentPaths().map((p) => byPath.get(p)).filter(Boolean);
  const head = el("div", { class: "home-head" },
    el("div", { class: "empty-ic" }, icon(c.id, 32)),
    el("div", {}, el("h2", {}, catLabel(c)), el("p", { class: "hint" }, fmtN("editor.home.count", entries.length))),
    el("div", { class: "grow" }),
    make ? el("button", { class: "primary", onclick: () => startNew(make) }, icon("plus", 16), t("editor.new")) : null);
  const page = el("div", { class: "page home" }, head);
  if (needs.length) page.append(card(t("editor.home.needs"), needs));
  else if (entries.length) page.append(el("p", { class: "home-ok" }, icon("check", 16), t("editor.home.all_good")));
  if (recent.length) page.append(card(t("editor.home.recent"), recent));
  if (!entries.length) page.append(el("p", { class: "hint" }, t("editor.select")));
  return page;
}
function fmtN(key, n) { return fmt(key, { n }); }

function drawPanel() {
  syncFocus();
  const p = $("panel");
  const keep = p.dataset.sel === (S.sel || "") ? p.scrollTop : 0;
  p.replaceChildren();
  p.dataset.sel = S.sel || "";
  if (!S.sel) { p.append(emptyState()); return; }
  const info = kindInfo(S.kind);
  const id = idOfPath(S.kind, S.sel);
  const page = el("div", { class: "page" + (S.kind === "map" ? " wide" : "") });
  page.append(pageHead(id), el("div", { id: "problems" }));
  if (info && id && info.texts.length) page.append(basicsCard(info.texts.map((x) => x.replace("{id}", id))));
  if (S.value !== null && S.value !== undefined) {
    const s = S.schemas[S.kind];
    if (S.kind === "map" && S.extra) page.append(el("section", { class: "card" }, mapPanel()));
    else if (S.kind === "dialogue") page.append(...dialogueEditor());
    else page.append(el("div", { class: "formcols" }, ...formOf(s, S.value)));
  } else {
    page.append(el("section", { class: "card" },
      el("textarea", { class: "code", spellcheck: "false", oninput: (e) => { S.raw = e.target.value; changed(); } }, S.raw ?? "")));
  }
  p.append(page);
  autosizeAll(page.querySelectorAll(".text-f textarea"));
  renderProblems();
  // The key switch only where the page has texts to wire.
  if (!page.querySelector(".textref")) page.querySelector(".keys-btn")?.remove();
  sectionBar(page);
  p.scrollTop = keep;
  if (id && !S.isNew) { usedBy(page, id, S.sel); contextCard(page, S.kind, id, S.sel).then(() => sectionBar(page)); }
}

// A long form gets a bar of its cards under the title: one click to a
// card, and a red mark on the ones with a problem.
function sectionBar(page) {
  const old = page.querySelector(".secbar");
  if (old) old.remove();
  const cards = [...page.querySelectorAll(".card")].filter((c) => c.querySelector(":scope > .card-head h3"));
  if (cards.length < 4) return;
  const bar = el("nav", { class: "secbar" }, cards.map((c) => el("button", {
    class: "secbtn" + (c.querySelector(".bad") ? " bad" : ""),
    onclick: () => c.scrollIntoView({ behavior: "smooth", block: "start" }),
  }, c.querySelector(":scope > .card-head h3").textContent)));
  page.querySelector(".page-head")?.append(bar);
}

// Where this id is named: the other files that would break, or at least
// need a look, if it changed. Loaded after the form so it never delays it.
async function usedBy(page, id, sel) {
  let found = [];
  try { found = await api("/api/refs?id=" + encodeURIComponent(id)); } catch (_) { return; }
  if (S.sel !== sel || !page.isConnected) return;
  const rows = found.filter((r) => r.path !== sel).map(entryOf2).sort((a, b) => a.kind.localeCompare(b.kind) || a.title.localeCompare(b.title));
  const card = el("section", { class: "card used" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.used_by")), el("span", { class: "count" }, rows.length)));
  if (!rows.length) card.append(el("p", { class: "hint" }, t("editor.used_none")));
  else {
    const grid = el("div", { class: "used-list" });
    for (const r of rows) {
      grid.append(el("button", { class: "entry slim", onclick: () => openUse(r, id) },
        thumb(r.title, r.pic, 28, r.kind),
        el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, r.title)),
        el("span", { class: "chip" }, t("editor.kind." + r.kind))));
    }
    card.append(grid);
  }
  page.append(card);
}

// A zone file opens its map, with the thing that names `id` selected.
async function openUse(r, id) {
  const list = { zone_npcs: "npcs", zone_monsters: "monsters", zone_nodes: "nodes" }[r.kind];
  if (!list) { await open(r.path); return; }
  await linkIndex();
  const hit = (S.links.all.find((f) => f.path === r.path)?.json || []).findIndex((x) => Object.values(x).includes(id));
  if (hit >= 0) await openOnMap(r.id, list, hit); else await open(r.path);
}

// A file the server listed by path only: its brief is in the file list.
function entryOf2(r) {
  const f = S.content.files.find((x) => x.path === r.path);
  return f ? entryOf(f) : { path: r.path, kind: r.kind, id: r.id, title: r.id || r.path, pic: null, state: "ok" };
}

function pageHead(id) {
  const kind = S.kind ?? "raw";
  const title = (id && titleOf(kind, id)) || id || t("editor.kind." + kind);
  const pic = S.value && typeof S.value === "object" && !Array.isArray(S.value) ? picOf(S.value) : null;
  return el("header", { class: "page-head" },
    thumb(title, pic, 52, kind),
    el("div", { class: "head-txt" },
      el("div", { class: "crumb" }, t("editor.kind." + kind), el("code", {}, id || S.sel)),
      el("h1", { id: "head-title" }, title)),
    el("div", { class: "grow" }),
    headTools(kind, id),
    el("span", { id: "state", class: "chip" }),
    el("button", { class: "primary", id: "save-btn", title: "Ctrl+S", onclick: () => submit(true) }, icon("check", 16), t("editor.save")));
}

// Small actions on the open file: show the text keys, copy it, change a
// new id. Each only where it makes sense.
function headTools(kind, id) {
  const info = kindInfo(kind);
  const box = el("div", { class: "head-tools" });
  if (S.kind !== "map") {
    box.append(el("button", { class: "ghost sm keys-btn" + (showKeys() ? " on" : ""), title: t("editor.keys_hint"),
      onclick: () => { storeSet("editor-ui.keys", showKeys() ? "0" : "1"); syncKeys(); drawPanel(); } }, t("editor.keys")));
  }
  if (info?.per_id && id && !S.isNew && kind !== "map") {
    box.append(el("button", { class: "ghost sm", onclick: () => startNew(kind, id) }, t("editor.duplicate")));
    if (isNewFile(S.sel)) box.append(el("button", { class: "ghost sm", title: t("editor.rename_hint"), onclick: renameId }, t("editor.rename")));
  }
  return box;
}

// Text keys are for whoever wires texts by hand: hidden unless asked for.
function showKeys() { return storeGet("editor-ui.keys") === "1"; }
function syncKeys() { $("app").classList.toggle("show-keys", showKeys()); }

// Every edit marks the page and asks the server to check it a moment
// later, so problems show up while editing instead of on save.
function changed() {
  recordUndo();
  renderState();
  clearTimeout(S.checkTimer);
  S.checkTimer = setTimeout(() => submit(false), 600);
}

function renderState() {
  const chip = $("state");
  if (!chip) return;
  const errors = S.saveReport?.errors.length ?? 0;
  let cls = "ok", text = t("editor.all_ok");
  if (S.saving) { cls = ""; text = t("editor.saving"); }
  else if (S.checking) { cls = ""; text = t("editor.checking"); }
  else if (errors) { cls = "err"; text = `${t("editor.problems")}: ${errors}`; }
  else if (dirty()) { cls = "warn"; text = t("editor.unsaved_short"); }
  chip.className = "chip " + cls;
  chip.textContent = text;
  const btn = $("save-btn");
  if (btn) btn.classList.toggle("pulse", dirty() && !errors);
}

function renderProblems() {
  const box = $("problems");
  if (!box) return;
  box.replaceChildren();
  const r = S.saveReport;
  if (r && r.errors.length) {
    box.append(el("div", { class: "alert err" },
      el("div", { class: "alert-head" }, icon("alert", 16), t("editor.fix_first")),
      el("ul", {}, r.errors.map((e) => el("li", {}, clip(e), fileLink(e))))));
  }
  if (r && r.warnings.length) {
    box.append(el("div", { class: "alert warn" }, el("ul", {}, r.warnings.map((w) => el("li", {}, clip(w), fileLink(w))))));
  }
  const bad = badFields();
  for (const n of document.querySelectorAll("#panel [data-k]")) n.classList.toggle("bad", bad(n.dataset.k));
  renderState();
}

// A problem about another file (moving a zone breaks the next one) gets a
// button that opens it. The loader starts its messages with the file.
function fileLink(msg) {
  const m = /^(?:(?:error|warning): )?((?:[\w-]+\/)+[\w-]+(?:\.ron)?)(?::|\s)/.exec(msg);
  if (!m) return null;
  let path = m[1];
  if (/^zones\/[^/]+$/.test(path)) path += "/map.ron";
  if (!path.endsWith(".ron")) return null;
  const here = S.sel && (path === S.sel || (S.kind === "map" && path.startsWith(S.sel.replace(/map\.ron$/, ""))));
  if (here || !S.content.files.some((f) => f.path === path)) return null;
  return el("button", { class: "link sm", onclick: () => open(path) }, t("editor.open"), " →");
}

// The file name repeats in every message of the open file; drop it.
// A problem in the editor's language (P12): the server cuts the English
// sentence into keys; a piece without a key stays as written. The leading
// error/warning and the open file are left out.
function clip(msg, from) {
  const parts = from?.[msg] ?? S.saveReport?.parts?.[msg] ?? S.report?.parts?.[msg];
  if (!parts) return msg.replace(/^(error|warning): /, "").replace(S.sel + ": ", "");
  return parts.filter((p, i) => !(i === 0 && (p.key === "error" || p.key === "warning")) && p.text !== S.sel)
    .map(problemPart).join(": ");
}

function problemPart(p) {
  if (p.text !== undefined) return p.text;
  const k = "editor.problem." + p.key;
  const args = Object.fromEntries(Object.entries(p.args).map(([n, v]) => [n, problemPart(v)]));
  return S.ui[k] ? fmt(k, args) : Object.values(args).join(" ");
}

// Fields the current problems name, so the form can point at them. The
// messages come from the game's loader, which names the field it refused.
function badFields() {
  const text = [...(S.saveReport?.errors || []), ...(S.saveReport?.warnings || [])].join(" ");
  const esc = (x) => x.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return (name) => name.length > 2 && new RegExp(`\\b${esc(name)}\\b`).test(text);
}

// ---------- name and description ----------

function basicsCard(keys) {
  const card = el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.basics")), el("span", { class: "card-hint" }, t("editor.name_hint"))));
  for (const key of keys) card.append(textField(key));
  return card;
}

function textField(key) {
  const row = rowOf(key);
  const pend = () => S.pendingTexts[key] || { text: row?.texts.en ?? "", note: row?.note ?? undefined };
  const isName = key.endsWith(".name"), isDesc = key.endsWith(".desc");
  const label = isName ? t("editor.name") : isDesc ? t("editor.description") : key;
  const value = textOf(key);
  const max = row?.max_chars;
  const counter = max ? el("span", { class: "counter" }, `${value.length}/${max}`) : null;
  const onText = (e) => {
    S.pendingTexts[key] = { ...pend(), text: e.target.value };
    if (counter) counter.textContent = `${e.target.value.length}/${max}`;
    if (isName && $("head-title")) $("head-title").textContent = e.target.value || idOfPath(S.kind, S.sel);
    if (e.target.tagName === "TEXTAREA") autosize(e.target);
    changed();
  };
  const input = isDesc
    ? el("textarea", { rows: 2, maxlength: max, oninput: onText }, value)
    : el("input", { class: isName ? "big" : "", value, maxlength: max, oninput: onText });
  const locales = S.meta.locales;
  const have = locales.filter((l) => row?.texts[l]);
  const missing = locales.filter((l) => !row?.texts[l]);
  const review = row?.review ?? [];
  const noteVal = pend().note ?? "";
  // The note is for translators: kept out of the way until asked for.
  const note = el("input", { class: "note-input", value: noteVal, placeholder: t("editor.note"), hidden: true,
    oninput: (e) => { S.pendingTexts[key] = { ...pend(), note: e.target.value }; changed(); } });
  const noteBtn = el("button", { class: "link", onclick: () => { note.hidden = false; noteBtn.hidden = true; note.focus(); } },
    "+ " + t("editor.note"));
  return el("div", { class: "f text-f", "data-k": key },
    el("label", {}, label),
    input,
    el("div", { class: "text-meta" },
      el("span", { class: "chip " + (missing.length ? "warn" : "ok"), title: missing.map((l) => langName(l)).join(", ") },
        fmt("editor.langs", { n: have.length, m: locales.length })),
      review.length ? el("span", { class: "chip warn", title: review.join(", ") }, t("editor.review")) : null,
      noteBtn, el("div", { class: "grow" }), counter),
    note);
}

function autosize(e) {
  e.style.height = "auto";
  e.style.height = e.scrollHeight + 2 + "px";
}

// Many at once: every write first, then every read, then every write, so
// the page is laid out once instead of once per box (hundreds of texts).
function autosizeAll(list) {
  const boxes = [...list];
  for (const a of boxes) a.style.height = "auto";
  const hs = boxes.map((a) => a.scrollHeight);
  boxes.forEach((a, i) => { a.style.height = hs[i] + 2 + "px"; });
}

// ---------- undo ----------
//
// Ctrl+Z and Ctrl+Shift+Z on a form or a dialogue (the map has its own).
// Edits less than a moment apart are one step; inside a text field the
// browser's own undo of that field comes first.

const UNDO_STEPS = 100;
function stateNow() { return JSON.stringify([S.value, S.raw, S.pendingTexts]); }
function resetUndo() { S.undo = []; S.redo = []; S.undoLast = stateNow(); S.undoAt = 0; }
function recordUndo() {
  if (S.restoring || S.kind === "map") return;
  S.undo ||= []; S.redo ||= [];
  const now = stateNow();
  if (now === S.undoLast) return;
  if (Date.now() - (S.undoAt || 0) > 700) {
    S.undo.push(S.undoLast);
    if (S.undo.length > UNDO_STEPS) S.undo.shift();
    S.redo = [];
  }
  S.undoLast = now; S.undoAt = Date.now();
}
function stepUndo(back) {
  const from = back ? S.undo : S.redo, to = back ? S.redo : S.undo;
  if (!from?.length) return;
  to.push(stateNow());
  const [v, raw, texts] = JSON.parse(from.pop());
  S.value = v; S.raw = raw; S.pendingTexts = texts;
  S.undoLast = stateNow(); S.undoAt = 0;
  S.restoring = true;
  drawPanel();
  changed();
  S.restoring = false;
}

// ---------- check and save ----------

async function submit(write) {
  if (!S.sel) return;
  clearTimeout(S.checkTimer);
  const body = { files: [], texts: [] };
  if (S.value !== null && S.value !== undefined) body.files.push({ path: S.sel, json: clean(S.value) });
  else body.files.push({ path: S.sel, text: S.raw ?? "" });
  if (S.kind === "map" && S.extra) body.files.push(...mapExtraFiles());
  for (const [key, p] of Object.entries(S.pendingTexts)) {
    if (p.text === undefined) continue;
    body.texts.push({ locale: "en", key, text: p.text, note: p.note || undefined, max_chars: p.max_chars });
  }
  const sel = S.sel;
  if (write) S.saving = true; else S.checking = true;
  renderState();
  try {
    const r = await api(write ? "/api/save" : "/api/check", body);
    if (S.sel !== sel) return;
    S.saveReport = r.report;
    if (write && r.ok) {
      S.isNew = false; S.pendingTexts = {};
      await Promise.all([loadTexts(), reloadContent()]);
      S.original = snap();
      toast(t("editor.saved"));
      drawRail(); drawSideList(); drawPanel();
    } else if (write) {
      toast(t("editor.fix_first"), "err");
    }
  } catch (e) {
    if (S.sel === sel) S.saveReport = { errors: [e.message], warnings: [], missing: {} };
    if (write) toast(e.message, "err");
  } finally {
    S.saving = false; S.checking = false;
  }
  renderProblems();
}

// Objects never carry null: an absent optional field is left out.
function clean(v) {
  if (Array.isArray(v)) return v.map(clean);
  if (v && typeof v === "object") {
    const o = {};
    for (const [k, x] of Object.entries(v)) if (x !== null && x !== undefined) o[k] = clean(x);
    return o;
  }
  return v;
}

// ---------- forms from JSON schema ----------

function R(root, s) {
  let guard = 0;
  while (s && s.$ref && guard++ < 20) s = root.$defs[s.$ref.split("/").pop()];
  return s || {};
}

function shape(root, s) {
  s = R(root, s);
  if (s.anyOf) return "option";
  if (Array.isArray(s.type) && s.type.includes("null")) return "option";
  if (s.oneOf || s.enum || s.const !== undefined) return "enum";
  if (s.type === "array" && s.prefixItems) return "tuple";
  if (s.type === "array") return "list";
  if (s.type === "object" && s.properties) return "struct";
  if (s.type === "object") return "map";
  if (s.type === "integer" || s.type === "number") return "number";
  if (s.type === "boolean") return "bool";
  return "string";
}

function inner(root, s) {
  s = R(root, s);
  if (s.anyOf) return s.anyOf.find((x) => R(root, x).type !== "null");
  return { ...s, type: s.type.find((x) => x !== "null") };
}

function variants(root, s) {
  s = R(root, s);
  const out = [];
  for (const v0 of s.oneOf || []) {
    const v = R(root, v0);
    if (v.const !== undefined) out.push({ name: v.const, payload: null, doc: v.description });
    else if (v.enum) for (const e of v.enum) out.push({ name: e, payload: null, doc: v.description });
    else if (v.properties) { const n = Object.keys(v.properties)[0]; out.push({ name: n, payload: v.properties[n], doc: v.description }); }
  }
  for (const e of s.enum || []) out.push({ name: e, payload: null });
  if (s.const !== undefined) out.push({ name: s.const, payload: null });
  return out;
}

function defaultOf(root, s) {
  const r = R(root, s);
  if (r.default !== undefined) return structuredClone(r.default);
  if (r["x-ref"] === "counter_family") return (S.content.ids.counter_family || [])[0] ?? "";
  if (s && s.default !== undefined) return structuredClone(s.default);
  switch (shape(root, s)) {
    case "option": return null;
    case "enum": { const v = variants(root, s)[0]; return !v ? "" : v.payload ? { [v.name]: defaultOf(root, v.payload) } : v.name; }
    case "tuple": return r["x-color"] ? [255, 200, 120] : r.prefixItems.map((x) => defaultOf(root, x));
    case "list": return [];
    case "map": return {};
    case "struct": {
      const o = {};
      for (const k of r.required || []) o[k] = defaultOf(root, r.properties[k]);
      return o;
    }
    // A count the rules want above zero starts at its own minimum when the
    // format states one; the check says the rest.
    case "number": return Math.max(0, r.minimum ?? 0);
    case "bool": return false;
    default: return "";
  }
}

// One line of the form: a number, a switch, a short choice, a reference.
function isSimple(root, s) {
  switch (shape(root, s)) {
    case "number": case "bool": case "string": return true;
    case "enum": return variants(root, s).every((v) => !v.payload);
    case "tuple": return R(root, s).prefixItems.every((x) => isSimple(root, x));
    case "option": return isSimple(root, inner(root, s));
    default: return false;
  }
}

// Redraws the content form; other screens draw themselves.
function redraw() { if (S.view === "content") drawPanel(); }

function help(root, s) {
  const d = R(root, s).description || s?.description;
  return d ? el("span", { class: "help", title: d }, "?") : null;
}

// The whole file: plain fields together in one card, and each group
// (equipment, loot, objectives...) in a card of its own.
function formOf(root, value) {
  const set = (v) => { S.value = v; changed(); };
  if (shape(root, root) !== "struct") return [el("section", { class: "card" }, field(root, root, value, set, ""))];
  return structParts(root, R(root, root), value, set, true);
}

// Long files of numbers split into cards by topic; a field not listed
// lands in the last card, so a new field never hides.
const FIELD_GROUPS = {
  progression_formulas: [
    ["level", ["max_level", "level_xp_base", "level_xp_exponent", "start_attribute", "free_points", "free_points_without_class"]],
    ["skills", ["prof_xp_base", "prof_xp_growth_permille", "prof_max", "technique_uses_base", "technique_max"]],
    ["resources", ["hp_base", "hp_per_vit", "hp_per_level", "vigor_base", "vigor_per_vit", "vigor_per_agi", "mp_base", "mp_per_int", "mp_per_level"]],
    ["regen", ["regen_interval_ticks", "hp_regen_permille", "vigor_regen_permille", "mp_regen_permille", "mp_regen_per_spi_permille", "out_of_combat_ticks", "rest_regen_permille"]],
    ["death", ["fallen_ticks", "newbie_level", "death_lock_secs"]],
    ["world", ["day_secs", "night_from_permille", "night_until_permille"]],
    ["companions", ["max_companions", "companion_rest_secs"]],
    ["quests", ["quests_per_rank"]],
  ],
  combat_formulas: [
    ["hit", ["hit_min_permille", "hit_max_permille", "hit_scale_permille", "variance_min_permille", "variance_max_permille"]],
    ["crit", ["crit_base_permille", "crit_per_dex_permille", "crit_max_permille", "crit_damage_permille"]],
    ["damage", ["attribute_coef_permille", "prof_damage_divisor", "level_diff_step_permille", "level_diff_cap_permille", "pvp_damage_permille", "unarmed_damage", "unarmed_attack_ticks"]],
    ["pace", ["global_cooldown_ticks", "support_per_point_permille"]],
  ],
  economy_formulas: [
    ["shops", ["sell_permille", "buy_permille", "npc_buy_max", "appraise_permille"]],
    ["wear", ["repair_permille", "repair_max_loss_permille", "durability_per_hit", "death_durability_permille", "warn_durability_permille"]],
    ["crafting", ["craft_xp_permille", "craft_quality_permille", "craft_quality_per_point_permille", "craft_quality_per_dex_permille", "craft_quality_max_permille", "gather_cooldown_ticks", "mastery_divisor", "mastery_max"]],
    ["rarity", ["rarity_power_permille", "rarity_durability_permille", "rarity_value_permille", "power_chance_permille", "hidden_chance_permille"]],
  ],
};

function structParts(root, r, value, set, top) {
  const groupsOf = top && FIELD_GROUPS[S.kind];
  if (groupsOf) {
    // One card per topic, each a struct of its own fields.
    const listed = new Set(groupsOf.flatMap(([, ks]) => ks));
    const rest = Object.keys(r.properties).filter((k) => !listed.has(k));
    const cards = [...groupsOf, ["other", rest]].filter(([, ks]) => ks.some((k) => k in r.properties));
    return cards.map(([g, ks]) => {
      const part = { ...r, properties: Object.fromEntries(ks.filter((k) => k in r.properties).map((k) => [k, r.properties[k]])) };
      return el("section", { class: "card" },
        el("div", { class: "card-head" }, el("h3", {}, t("editor.fgroup." + g))),
        ...structParts(root, part, value, set, false));
    });
  }
  const grid = el("div", { class: "fields" });
  const groups = [];
  // A counter key names something only the family next to it knows, so
  // the two fields of a counter are drawn together.
  const keyField = Object.keys(r.properties).find((k) => R(root, r.properties[k])["x-ref"] === "counter_key");
  for (const [k, ps] of Object.entries(r.properties)) {
    const v = value[k] !== undefined ? value[k] : defaultOf(root, ps);
    let sub = (nv) => { value[k] = nv; set(value); };
    const ref = R(root, ps)["x-ref"];
    if (ref === "counter_family" && keyField) {
      sub = (nv) => { value[k] = nv; value[keyField] = ""; set(value); redraw(); };
    }
    if (ref === "counter_key") {
      const kind = S.meta.counter_keys[value.family ?? ""];
      // A family with no keys only counts its total: nothing to choose.
      if (!kind && !v) continue;
      grid.append(el("div", { class: "f", "data-k": k },
        el("label", {}, kind ? flabel(kind) : flabel(k)), counterKeyInput(kind, v, sub)));
      continue;
    }
    if (isSimple(root, ps)) {
      grid.append(el("div", { class: "f", "data-k": k },
        el("label", {}, flabel(k), help(root, ps)), field(root, ps, v, sub, k)));
    } else {
      groups.push(group(root, k, ps, v, sub, top));
    }
  }
  const out = [];
  if (grid.childElementCount) {
    out.push(top ? el("section", { class: "card" },
      el("div", { class: "card-head" }, el("h3", {}, t("editor.properties"))), grid) : grid);
  }
  return out.concat(groups);
}

function group(root, k, ps, v, set, top) {
  const box = el("section", { class: top ? "card" : "sub", "data-k": k });
  const head = el("div", { class: "card-head" }, el("h3", {}, flabel(k)), help(root, ps));
  box.append(head);
  if (shape(root, ps) === "option") {
    const on = v !== null && v !== undefined;
    head.append(el("div", { class: "grow" }), toggle(on, (c) => { set(c ? defaultOf(root, inner(root, ps)) : null); redraw(); }));
    if (on) box.append(...body(root, inner(root, ps), v, set, k));
    else box.classList.add("off");
    return box;
  }
  box.append(...body(root, ps, v, set, k));
  return box;
}

function body(root, s, v, set, k) {
  if (shape(root, s) === "struct") return structParts(root, R(root, s), v, set, false);
  return [field(root, s, v, set, k)];
}

function toggle(on, onchange) {
  return el("label", { class: "switch" },
    el("input", { type: "checkbox", checked: on, onchange: (e) => onchange(e.target.checked) }), el("span", {}));
}

function delBtn(onclick) {
  return el("button", { class: "icon-btn danger", title: t("editor.remove"), onclick }, icon("trash", 16));
}

function field(root, s, value, set, k) {
  const r = R(root, s);
  switch (shape(root, s)) {
    case "struct":
      return el("div", { class: "stack" }, ...structParts(root, r, value, set, false));
    case "option": {
      const on = value !== null && value !== undefined;
      const box = el("div", { class: "opt-f" }, toggle(on, (c) => { set(c ? defaultOf(root, inner(root, s)) : null); redraw(); }));
      if (on) box.append(field(root, inner(root, s), value, set, k));
      return box;
    }
    case "enum": return enumField(root, s, value, set, k);
    case "tuple": {
      // An RGB color (0..=255 each): a picker, with the light it gives.
      if (r["x-color"]) {
        const hex = (v) => "#" + (v || [255, 255, 255]).map((c) => Number(c || 0).toString(16).padStart(2, "0")).join("");
        return el("input", { type: "color", class: "colorf", value: hex(value),
          oninput: (e) => { const h = e.target.value; set([1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16))); } });
      }
      const row = el("div", { class: "tuple" });
      r.prefixItems.forEach((ps, i) => {
        if (i) row.append(el("span", { class: "dash" }, "–"));
        row.append(field(root, ps, value?.[i], (nv) => { value[i] = nv; set(value); }, `${k}.${i}`));
      });
      return row;
    }
    case "list": return listField(root, r, value, set, k);
    case "map": return mapField(root, r, value, set, k);
    case "number": return numberField(r, value, set, k);
    case "bool":
      return toggle(!!value, (c) => set(c));
    default:
      return refInput(r["x-ref"], value ?? "", set, r);
  }
}

// ---------- numbers ----------

function numberField(r, value, set, k) {
  const unit = el("span", { class: "unit" });
  const show = (n) => { unit.textContent = unitOf(k, n); };
  const bad = (n) => !Number.isFinite(n) || (r.type === "integer" && !Number.isInteger(n))
    || (r.minimum !== undefined && n < r.minimum) || (r.maximum !== undefined && n > r.maximum);
  const inp = el("input", { type: "number", value: value ?? 0, min: r.minimum, max: r.maximum, step: r.type === "integer" ? 1 : "any",
    oninput: (e) => { const n = Number(e.target.value); inp.classList.toggle("bad", bad(n)); show(n); set(n); } });
  show(value ?? 0);
  return el("div", { class: "num" }, inp, unit);
}

// A count of ticks, thousandths or seconds read the way a designer
// thinks of it. Only arithmetic: the tick rate comes from the server.
function unitOf(k, n) {
  if (!Number.isFinite(n)) return "";
  if (/_ticks$/.test(k)) return "≈ " + duration(n / S.meta.ticks_per_second);
  if (/_permille$/.test(k) || k === "chance") return "= " + (n / 10).toLocaleString(S.uiLocale, { maximumFractionDigits: 1 }) + "%";
  if (/_secs$/.test(k)) return "≈ " + duration(n);
  return "";
}

function duration(s) {
  const f = (x) => x.toLocaleString(S.uiLocale, { maximumFractionDigits: 1 });
  if (s >= 3600) return f(s / 3600) + " h";
  if (s >= 60) return f(s / 60) + " min";
  return f(s) + " s";
}

// ---------- choices ----------

function enumField(root, s, value, set) {
  const vs = variants(root, s);
  const cur = typeof value === "string" ? value : value && typeof value === "object" ? Object.keys(value)[0] : "";
  const anyPayload = vs.some((v) => v.payload);
  const choose = (name) => {
    const v = vs.find((x) => x.name === name);
    set(v.payload ? { [v.name]: defaultOf(root, v.payload) } : v.name);
    if (anyPayload) redraw();
  };
  const short = vs.length <= 4 && vs.every((v) => vlabel(v.name).length <= 16);
  let chooser;
  if (short) {
    chooser = el("div", { class: "seg" }, vs.map((v) => el("button", {
      class: v.name === cur ? "on" : "", title: v.doc || "",
      onclick: (e) => {
        if (v.name === cur && anyPayload) return;
        for (const b of chooser.children) b.classList.toggle("on", b === e.currentTarget);
        choose(v.name);
      },
    }, vlabel(v.name))));
  } else {
    chooser = el("select", { onchange: (e) => choose(e.target.value) },
      vs.map((v) => el("option", { value: v.name, selected: v.name === cur, title: v.doc || "" }, vlabel(v.name))));
  }
  const v = vs.find((x) => x.name === cur);
  if (!v || !v.payload) return chooser;
  return el("div", { class: "variant" }, chooser,
    el("div", { class: "variant-body" }, ...body(root, v.payload, value[cur], (nv) => { value[cur] = nv; set(value); }, cur)));
}

// ---------- lists ----------

// Items short enough to be a row of a table: a few plain fields.
function columns(root, it) {
  const r = R(root, it);
  const sh = shape(root, it);
  if (sh === "struct") {
    const props = Object.entries(r.properties);
    if (props.length > 5 || !props.every(([, ps]) => isSimple(root, ps))) return null;
    return props.map(([k, ps]) => ({ key: k, label: flabel(k), schema: ps,
      get: (item) => item[k] !== undefined ? item[k] : defaultOf(root, ps), put: (item, v) => { item[k] = v; } }));
  }
  if (sh === "tuple" && r.prefixItems.every((x) => isSimple(root, x))) {
    return r.prefixItems.map((ps, i) => ({ key: `${i}`, label: R(root, ps)["x-ref"] ? flabel(R(root, ps)["x-ref"]) : "", schema: ps,
      get: (item) => item[i], put: (item, v) => { item[i] = v; } }));
  }
  return null;
}

// A list of names of existing things, or of fixed choices: tags with an
// "add" list that offers only what is not in yet, so nothing repeats.
function chipsField(root, r, list, set) {
  const ir = R(root, r.items);
  const ref = ir["x-ref"];
  const box = el("div", { class: "chips" });
  if (!ref) {
    const vs = ir.enum || variants(root, r.items).map((v) => v.name);
    for (const v of vs) {
      const on = list.includes(v);
      box.append(el("button", { class: "chip-pick toggle" + (on ? " on" : ""), onclick: () => {
        const i = list.indexOf(v);
        if (i >= 0) list.splice(i, 1); else list.push(v);
        set(list); redraw();
      } }, icon(on ? "check" : "plus", 13), vlabel(v)));
    }
    return box;
  }
  const ids = (S.content.ids[ref] || []).filter((id) => !(ref === S.kind && id === selId()));
  const title = (id) => refTitle(ref, id);
  list.forEach((v, i) => {
    const bad = !ids.includes(v);
    box.append(el("span", { class: "chip-pick" + (bad ? " bad" : ""), title: bad ? t("editor.unknown_value") : "" },
      bad ? `${v} — ${t("editor.unknown_value")}` : title(v),
      el("button", { class: "x", title: t("editor.remove"), onclick: () => { list.splice(i, 1); set(list); redraw(); } }, icon("x", 12))));
  });
  const rest = ids.filter((id) => !list.includes(id)).map((id) => ({ id, title: title(id) }))
    .sort((a, b) => a.title.localeCompare(b.title));
  if (rest.length) {
    box.append(el("select", { class: "chip-add", onchange: (e) => {
      if (!e.target.value) return;
      list.push(e.target.value); set(list); redraw();
    } }, el("option", { value: "" }, "+ " + t("editor.add")), rest.map((o) => el("option", { value: o.id }, o.title))));
  }
  return box;
}

function chipsFit(root, r) {
  const ir = R(root, r.items || {});
  const ref = ir["x-ref"];
  if (ref) return !["text", "node", "sprite", "icon", "counter_family", "counter_key"].includes(ref);
  if (!r.uniqueItems) return false;
  if (ir.enum) return true;
  const vs = ir.oneOf ? variants(root, r.items) : [];
  return vs.length > 0 && vs.every((v) => !v.payload);
}

function listField(root, r, value, set, k) {
  const list = Array.isArray(value) ? value : [];
  if (chipsFit(root, r)) return chipsField(root, r, list, set);
  const it = r.items;
  const box = el("div", { class: "list-f" });
  const cols = columns(root, it);
  const scalar = !cols && isSimple(root, it);
  if (list.length && (cols || scalar)) {
    const n = cols ? cols.length : 1;
    const table = el("div", { class: "rows", style: `--cols:${n}` });
    if (cols) table.append(el("div", { class: "rows-head" }, cols.map((c) => el("span", {}, c.label)), el("span", {})));
    list.forEach((item, i) => {
      const cells = cols
        ? cols.map((c) => el("div", { class: "cell", "data-k": c.key },
          field(root, c.schema, c.get(item), (nv) => { c.put(item, nv); set(list); }, c.key)))
        : [el("div", { class: "cell" }, field(root, it, item, (nv) => { list[i] = nv; set(list); }, k))];
      table.append(el("div", { class: "rows-row" }, ...cells, delBtn(() => { list.splice(i, 1); set(list); redraw(); })));
    });
    box.append(table);
  } else {
    // Each item reads as one sentence; a click opens it for editing. A new
    // item, and one the problems name, start open.
    S.openItems ||= new Set();
    list.forEach((item, i) => {
      const id = `${S.sel}|${k}|${i}`;
      const line = sentence(root, it, item);
      const opened = !line || S.openItems.has(id);
      const head = el("div", { class: "item-head" + (opened ? "" : " closed"), onclick: (e) => {
        if (e.target.closest("button.icon-btn")) return;
        if (S.openItems.has(id)) S.openItems.delete(id); else S.openItems.add(id);
        redraw();
      } }, el("span", { class: "idx" }, i + 1),
        line ? el("span", { class: "item-line" }, line) : null,
        el("div", { class: "grow" }), line ? el("span", { class: "chev" + (opened ? " on" : "") }, icon("chevron", 14)) : null,
        delBtn(() => { list.splice(i, 1); S.openItems.delete(id); set(list); redraw(); }));
      box.append(el("div", { class: "item-card" + (opened ? "" : " closed") }, head,
        ...(opened ? body(root, it, item, (nv) => { list[i] = nv; set(list); }, k) : [])));
    });
  }
  box.append(el("button", { class: "add", onclick: () => {
    list.push(defaultOf(root, it));
    (S.openItems ||= new Set()).add(`${S.sel}|${k}|${list.length - 1}`);
    set(list); redraw();
  } }, icon("plus", 16), t("editor.add")));
  return box;
}

// One item of a list as a sentence: its kind ("Talk to", "Item") and its
// values, names in place of ids. Empty when the item is not a choice of
// kinds with plain values (then it is always shown open).
function sentence(root, s, v) {
  if (shape(root, s) !== "enum" || v === null || v === undefined) return "";
  const name = typeof v === "string" ? v : Object.keys(v)[0];
  const vr = variants(root, s).find((x) => x.name === name);
  if (!vr) return "";
  if (!vr.payload) return vlabel(name);
  const parts = [];
  const add = (ps, x) => {
    const r = R(root, ps);
    switch (shape(root, ps)) {
      case "struct": for (const [k, q] of Object.entries(r.properties)) if (x?.[k] !== undefined) add(q, x[k]); return true;
      case "tuple": r.prefixItems.forEach((q, i) => add(q, x?.[i])); return true;
      case "number": parts.push(typeof x !== "number" ? "" : (parts.length ? "×" : "") + x.toLocaleString(S.uiLocale)); return true;
      case "bool": return true;
      case "enum": if (typeof x === "string") parts.push(vlabel(x)); return true;
      case "option": if (x !== null && x !== undefined) add(inner(root, ps), x); return true;
      case "string": parts.push(r["x-ref"] && r["x-ref"] !== "text" ? titleOf(r["x-ref"], x) || "—" : x || "—"); return true;
      default: return false;
    }
  };
  if (!add(vr.payload, v[name])) return "";
  return `${vlabel(name)}: ${parts.filter(Boolean).join(" ")}`;
}

function mapField(root, r, value, set) {
  const map = value && typeof value === "object" ? value : {};
  const box = el("div", { class: "list-f" });
  for (const key of Object.keys(map)) {
    box.append(el("div", { class: "item-card" },
      el("div", { class: "item-head" }, el("code", { class: "key" }, key), el("div", { class: "grow" }),
        delBtn(() => { delete map[key]; set(map); redraw(); })),
      ...body(root, r.additionalProperties, map[key], (nv) => { map[key] = nv; set(map); }, key)));
  }
  const name = el("input", { class: "mono", placeholder: t("editor.id") });
  const add = () => {
    const key = name.value.trim();
    if (!key || /\s/.test(key) || map[key] !== undefined) { name.classList.add("bad"); return; }
    map[key] = defaultOf(root, r.additionalProperties); set(map); redraw();
  };
  name.addEventListener("keydown", (e) => { if (e.key === "Enter") add(); });
  box.append(el("div", { class: "add-row" }, name, el("button", { class: "add inline", onclick: add }, icon("plus", 16), t("editor.add"))));
  return box;
}

// ---------- references ----------

// A string that names something else. A name that has to exist already is
// a list to choose from, never free typing: a designer should not have to
// know how an id is spelled. A text key is the exception — a new key is
// written here — so it stays a field with suggestions.
function refInput(ref, value, set, r) {
  if (!ref) {
    if (/#rrggbb/.test(r?.description || "")) return colorInput(value, set);
    return el("input", { value, oninput: (e) => set(e.target.value) });
  }
  if (ref === "text") return textRefInput(value, set, keyPrefix(r));
  if (ref === "counter_family") {
    const fams = S.content.ids.counter_family || [];
    const unknown = value !== "" && !fams.includes(value);
    const sel = el("select", { onchange: (e) => set(e.target.value) },
      value === "" ? el("option", { value: "", selected: true }, "— " + t("editor.pick")) : null,
      unknown ? el("option", { value, selected: true }, `${value} — ${t("editor.unknown_value")}`) : null,
      fams.map((f) => el("option", { value: f, selected: f === value }, vlabel(f))));
    sel.classList.toggle("bad", unknown);
    return sel;
  }
  if (ref === "sprite" || ref === "icon") return artPicker(ref, value, set);
  const ids = ref === "node" ? Object.keys((S.value && S.value.nodes) || {}) : (S.content.ids[ref] || []);
  const opts = ids.map((id) => ({ id, title: ref === "node" ? id : refTitle(ref, id) })).sort((a, b) => a.title.localeCompare(b.title));
  const unknown = value !== "" && !ids.includes(value);
  const target = kindInfo(ref);
  const go = el("button", { class: "icon-btn", title: t("editor.open"), disabled: !value || unknown,
    onclick: () => { if (sel.value) open(target.path.replace("{id}", sel.value)); } }, icon("arrow", 16));
  const sel = el("select", { onchange: (e) => { set(e.target.value); sel.classList.remove("bad"); go.disabled = !e.target.value; } },
    el("option", { value: "", selected: value === "" }, "— " + t("editor.pick")),
    // A name already in the file that no longer exists stays visible and
    // marked, instead of being silently changed to something else.
    unknown ? el("option", { value, selected: true }, `${value} — ${t("editor.unknown_value")}`) : null,
    opts.map((o) => el("option", { value: o.id, selected: o.id === value }, o.title)));
  sel.classList.toggle("bad", unknown);
  return target && target.per_id ? el("div", { class: "ref" }, sel, go) : sel;
}

// Which monster, quest... a counter counts; empty is the family total.
function counterKeyInput(kind, value, set) {
  const ids = kind ? (S.content.ids[kind] || []) : [];
  const opts = ids.map((id) => ({ id, title: refTitle(kind, id) })).sort((a, b) => a.title.localeCompare(b.title));
  const unknown = value !== "" && !ids.includes(value);
  const sel = el("select", { onchange: (e) => set(e.target.value) },
    el("option", { value: "", selected: value === "" }, t("editor.counter_total")),
    unknown ? el("option", { value, selected: true }, `${value} — ${t("editor.unknown_value")}`) : null,
    opts.map((o) => el("option", { value: o.id, selected: o.id === value }, o.title)));
  sel.classList.toggle("bad", unknown);
  return sel;
}

function colorInput(value, set) {
  const ok = (v) => /^#[0-9a-f]{6}$/i.test(v);
  const pick = el("input", { type: "color", value: ok(value) ? value : "#808080",
    oninput: (e) => { text.value = e.target.value; set(e.target.value); } });
  const text = el("input", { class: "mono", value, placeholder: "#rrggbb",
    oninput: (e) => { set(e.target.value); if (ok(e.target.value)) pick.value = e.target.value; } });
  return el("div", { class: "ref" }, pick, text);
}

// The id of the file open in the form, from its path.
function selId() {
  const k = kindInfo(S.kind);
  if (!k || !S.sel) return "";
  const [pre, post] = k.path.split("{id}");
  return S.sel.startsWith(pre) && S.sel.endsWith(post ?? "") ? S.sel.slice(pre.length, S.sel.length - (post ?? "").length) : "";
}

// Where a new text key of this field starts: the schema may say
// (`x-key-prefix`); a dialogue line belongs to its NPC, as written by hand.
function keyPrefix(r) {
  if (r && r["x-key-prefix"]) return r["x-key-prefix"];
  const id = selId();
  if (S.kind === "dialogue") return `npc.${id}.`;
  return id ? `${S.kind}.${id}.` : `${S.kind}.`;
}

// A key nobody uses yet, made from the English text: whoever writes the
// line never has to invent a key.
function newKey(prefix, text) {
  const base = prefix + (slug(text).split("_").slice(0, 4).join("_") || "text");
  const used = (k) => S.texts.some((r) => r.key === k) || !!S.pendingTexts[k];
  let k = base;
  for (let i = 2; used(k); i++) k = `${base}_${i}`;
  return k;
}

function textRefInput(value, set, prefix) {
  const listId = "dl-" + Math.random().toString(36).slice(2);
  const opts = S.texts.map((r) => r.key);
  const known = (v) => v === "" || opts.includes(v) || !!S.pendingTexts[v];
  // Typed into an empty line: the key is made once, then follows nothing.
  let made = "";
  const en = el("input", { value: textOf(value), placeholder: t("editor.text_en"),
    oninput: (e) => {
      if ((!key.value || key.value === made) && !opts.includes(key.value)) {
        if (made) delete S.pendingTexts[made];
        made = e.target.value.trim() ? newKey(prefix || "", e.target.value) : "";
        key.value = made;
        set(made);
      }
      const k = key.value;
      if (!k) return;
      S.pendingTexts[k] = { ...S.pendingTexts[k], text: e.target.value };
      key.classList.toggle("bad", !known(k));
      changed();
    } });
  const key = el("input", { class: "mono key-input", value, list: listId, placeholder: t("editor.field.key"),
    oninput: (e) => { set(e.target.value); key.classList.toggle("bad", !known(e.target.value)); en.value = textOf(e.target.value); } });
  key.classList.toggle("bad", !known(value));
  return el("div", { class: "textref" }, en, key, el("datalist", { id: listId }, opts.map((o) => el("option", { value: o }))));
}

// ---------- pictures ----------

function artPicker(kind, value, set) {
  const known = (S.content.ids[kind] || []).includes(value);
  const btn = el("button", { class: "art-pick" + (value && !known ? " bad" : ""),
    onclick: async () => {
      const v = await chooseArt(kind, value);
      if (v === null || v === undefined || v === value) return;
      set(v);
      // A form redraws itself; elsewhere (floors screen, a dialog) only
      // this button changes: redrawing the content panel there would
      // blank the page.
      if (S.view === "content" && !btn.closest(".modal")) redraw();
      else if (btn.isConnected) btn.replaceWith(artPicker(kind, v, set));
    } },
    el("div", { class: "art-thumb" }, value && known
      ? (kind === "sprite" && S.art ? frameThumb(value, 28) : el("img", { src: artUrl(kind, value), alt: "" })) : icon("image", 18)),
    el("span", { class: "art-name" }, value ? (known ? value : `${value} — ${t("editor.unknown_value")}`) : t("editor.no_image")),
    icon("chevron", 14));
  return btn;
}

async function chooseArt(kind, current) {
  if (!S.art) await loadArt();
  let close = null;
  let q = "";
  const grid = el("div", { class: "art-grid" });
  const cell = (name, img) => el("button", { class: "art-cell" + (name === current ? " on" : ""), onclick: () => close(name) },
    el("div", { class: "frame" }, img), el("span", {}, name || t("editor.no_image")));
  const fill = () => {
    const imgs = S.art.kinds[kind].images.filter((i) => !q || norm(i.name).includes(q) || norm(i.folder).includes(q));
    grid.replaceChildren(cell("", icon("x", 22)),
      ...imgs.map((i) => cell(i.name, kind === "sprite" ? frameThumb(i.name, 72) : el("img", { src: artUrl(kind, i.name), alt: "", loading: "lazy" }))));
  };
  const folder = S.art.kinds[kind].images.find((i) => i.name === current)?.folder ?? S.art.kinds[kind].folders[0] ?? "";
  const up = el("button", { class: "ghost", onclick: async () => {
    const f = await pickFile();
    if (!f) return;
    const name = await upload(kind, folder, f);
    if (name) close(name);
  } }, icon("upload", 16), t("editor.art.upload"));
  fill();
  return dialog(t("editor.choose_image"), [
    el("div", { class: "picker-bar" }, searchBox("", (v) => { q = norm(v); fill(); }), up), grid,
  ], [], { wide: true, onReady: (c) => { close = c; } });
}

// ---------- graphics ----------

function drawArtSide() {
  const side = $("side");
  side.replaceChildren();
  side.append(el("div", { class: "side-head" }, el("h2", {}, t("editor.art"))));
  side.append(searchBox(S.filter, (v) => { S.filter = v; drawArt(); }));
  const idle = S.art.kinds[S.artKind].images.filter((i) => !S.art.kinds[S.artKind].used.includes(i.name)).length;
  side.append(el("button", { class: "chip-pick toggle" + (S.artUnused ? " on" : ""),
    onclick: () => { S.artUnused = !S.artUnused; drawArtSide(); drawArt(); } }, `${t("editor.art.only_unused")} (${idle})`));
  const broken = S.art.kinds[S.artKind].bad_strips || [];
  if (broken.length || S.artBroken) side.append(el("button", { class: "chip-pick toggle err" + (S.artBroken ? " on" : ""),
    onclick: () => { S.artBroken = !S.artBroken; drawArtSide(); drawArt(); } }, `${t("editor.looks.only_broken")} (${broken.length})`));
  const list = el("div", { class: "list" });
  for (const [kind, label] of [["sprite", t("editor.art.sprites")], ["icon", t("editor.art.icons")]]) {
    list.append(el("button", { class: "entry" + (S.artKind === kind ? " sel" : ""), onclick: () => { S.artKind = kind; S.artFolder = null; draw(); } },
      el("div", { class: "thumb letter", style: "--s:36px" }, icon(kind === "sprite" ? "npc" : "image", 18)),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, label)),
      el("span", { class: "count" }, S.art.kinds[kind].images.length)));
  }
  const data = S.art.kinds[S.artKind];
  if (data.folders.length > 1) {
    list.append(el("div", { class: "list-group" }, t("editor.art.folder")));
    const folderBtn = (f, label, n) => el("button", { class: "entry slim" + (S.artFolder === f ? " sel" : ""),
      onclick: () => { S.artFolder = f; drawArtSide(); drawArt(); } },
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, label)), el("span", { class: "count" }, n));
    list.append(folderBtn(null, t("editor.all_folders"), data.images.length));
    for (const f of data.folders) list.append(folderBtn(f, f || t("editor.art.root"), data.images.filter((i) => i.folder === f).length));
  }
  side.append(list);
}

function drawArt() {
  const p = $("panel");
  p.replaceChildren();
  const kind = S.artKind;
  const data = S.art.kinds[kind];
  const q = norm(S.filter);
  const usedNames = new Set(data.used);
  const rows = data.images.filter((i) => (S.artFolder === null || i.folder === S.artFolder)
    && (!S.artUnused || !usedNames.has(i.name))
    && (!S.artBroken || (data.bad_strips || []).includes(i.name))
    && (!q || norm(i.name).includes(q) || norm(i.folder).includes(q)));
  const page = el("div", { class: "page wide" });
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, t("editor.art")),
      el("h1", {}, kind === "sprite" ? t("editor.art.sprites") : t("editor.art.icons"))),
    el("div", { class: "grow" }), el("span", { class: "chip" }, `${rows.length} / ${data.images.length}`)));
  page.append(uploadBox(kind, data.folders));
  const broken = data.bad_strips || [];
  if (broken.length) page.append(el("div", { class: "alert warn" }, t("editor.looks.broken_alert").replace("{n}", broken.length), " ",
    el("button", { class: "ghost sm", onclick: () => { S.artBroken = true; drawArtSide(); drawArt(); } }, t("editor.looks.show_broken"))));
  if (kind === "sprite" && S.artFolder) {
    page.append(el("button", { class: "ghost sm", onclick: async () => { if (await layoutDialog(S.artFolder, true)) drawArt(); } },
      `${t("editor.looks.folder_layout")}: ${S.artFolder}`));
  }
  // The report already warns about a name with no image; this is the other
  // direction: an image no content names.
  const used = new Set(data.used);
  const grid = el("div", { class: "gallery" });
  for (const img of rows) {
    const idle = !used.has(img.name);
    // A sprite card opens its anchor and animation (phase 11).
    const layout = kind === "sprite" ? async () => { if (await layoutDialog(img.name)) { drawArt(); } } : null;
    grid.append(el("div", { class: "gcard" + (idle ? " unused" : "") + (layout ? " click" : ""), onclick: layout, title: layout ? t("editor.looks.edit_layout") : null },
      el("div", { class: "frame" }, kind === "sprite" ? frameThumb(img.name, 96) : el("img", { src: artUrl(kind, img.name), alt: img.name, loading: "lazy" }),
        el("button", { class: "icon-btn over", title: t("editor.art.replace"),
          onclick: (e) => { e.stopPropagation(); pickFile().then((f) => f && upload(kind, img.folder, f, img.name)); } }, icon("upload", 16))),
      el("div", { class: "gname" }, img.name),
      el("div", { class: "gmeta" }, `${img.width}×${img.height} · ${img.folder || t("editor.art.root")}`
        + (strip(img.name) ? ` · ${strip(img.name)}` : "")),
      idle ? el("span", { class: "chip warn" }, t("editor.art.unused")) : null,
      (data.bad_strips || []).includes(img.name) ? el("span", { class: "chip err", title: t("editor.looks.broken_hint") }, t("editor.looks.broken")) : null));
  }
  page.append(grid);
  p.append(page);
}

// "4 frames × 4 directions" for a strip, empty for a still image.
function strip(name) {
  const [f, d] = S.art.kinds.sprite.strips?.[name] || [1, 1];
  if (f === 1 && d === 1) return "";
  return t("editor.looks.strip_n").replace("{f}", f).replace("{d}", d);
}

function uploadBox(kind, folders) {
  const all = [...folders, ...(folders.includes("") ? [] : [""])];
  const folder = el("select", {}, all.map((f) => el("option", { value: f, selected: f === S.artFolder }, f || t("editor.art.root"))));
  const drop = el("div", { class: "drop", onclick: () => pickFile().then((f) => f && upload(kind, folder.value, f)) },
    el("div", { class: "drop-ic" }, icon("upload", 22)),
    el("div", { class: "drop-txt" }, el("strong", {}, t("editor.art.upload")), el("span", {}, t("editor.art.drop"))),
    el("label", { class: "drop-folder", onclick: (e) => e.stopPropagation() }, t("editor.art.folder"), folder));
  drop.addEventListener("dragover", (e) => { e.preventDefault(); drop.classList.add("over"); });
  drop.addEventListener("dragleave", () => drop.classList.remove("over"));
  drop.addEventListener("drop", (e) => {
    e.preventDefault();
    drop.classList.remove("over");
    const f = e.dataTransfer.files[0];
    if (f) upload(kind, folder.value, f);
  });
  return drop;
}

function pickFile() {
  return new Promise((resolve) => {
    const inp = el("input", { type: "file", accept: "image/png", hidden: true });
    inp.addEventListener("change", () => { resolve(inp.files[0] || null); inp.remove(); });
    inp.addEventListener("cancel", () => { resolve(null); inp.remove(); });
    document.body.append(inp);
    inp.click();
  });
}

// The file name is the name content calls the graphic by, so a chosen file
// decides the name unless an existing card asked to be replaced. Resolves
// with the saved name, or null.
async function upload(kind, folder, file, replaceName) {
  const name = el("input", { class: "mono", value: replaceName || slug(file.name.replace(/\.png$/i, "")) });
  const replace = el("input", { type: "checkbox", checked: !!replaceName });
  const url = URL.createObjectURL(file);
  const body = [
    el("div", { class: "upload-top" }, el("div", { class: "frame" }, el("img", { src: url, alt: "" })),
      el("div", { class: "fld grow" }, el("label", {}, t("editor.art.name")), name,
        el("label", { class: "check" }, replace, t("editor.art.replace")))),
  ];
  // A sprite says what it is right here: still image or strip (phase 11).
  const fmt = kind === "sprite" ? await uploadFormat(url, folder) : null;
  if (fmt) body.push(fmt.box);
  const ok = await dialog(t("editor.art.upload"), body, [
    { label: t("editor.cancel"), value: false },
    ...(fmt ? [{ label: t("editor.looks.upload_adjust"), value: "adjust" }] : []),
    { label: t("editor.art.upload"), value: true, primary: true },
  ], fmt ? { wide: true } : {});
  URL.revokeObjectURL(url);
  if (!ok) return null;
  const n = slug(name.value);
  if (!n) { await say(t("editor.art.upload"), t("editor.bad_id")); return null; }
  const path = `/api/art/${kind}/${encodeURIComponent(folder || "-")}/${encodeURIComponent(n)}` + (replace.checked ? "?replace=true" : "");
  try {
    const r = await api(path, file, true);
    toast(`${t("editor.art.saved")}: ${r.path} (${r.width}×${r.height})`);
    // The folder size check is for still images; a strip was chosen above.
    if (r.warnings.length && !(fmt && fmt.value().frames * fmt.value().directions > 1)) await say(t("editor.art.upload"), r.warnings.join(" · "));
    await Promise.all([loadArt(), reloadContent()]);
    if (fmt) await saveFormat(n, folder, fmt.value());
    if (ok === "adjust") await layoutDialog(n);
    if (S.view === "art") { drawArtSide(); drawArt(); }
    return n;
  } catch (e) {
    toast(e.message, "err");
    return null;
  }
}

// ---------- translations ----------

// The content a text key belongs to: a name or description key comes from
// the id (`item.<id>.name`); any other key is written in some file.
async function openOwner(key) {
  if (dirty() && !(await askLeave())) return;
  for (const k of S.meta.kinds) {
    for (const tpl of k.texts) {
      const [pre, post] = tpl.split("{id}");
      if (key.startsWith(pre) && key.endsWith(post)) {
        const id = key.slice(pre.length, key.length - post.length);
        if ((S.content.ids[k.name] || []).includes(id)) { S.pendingTexts = {}; S.isNew = false; S.sel = null; await open(k.path.replace("{id}", id)); return; }
      }
    }
  }
  const hits = await api("/api/refs?id=" + encodeURIComponent(key));
  if (hits.length) { S.pendingTexts = {}; S.sel = null; await open(hits[0].path); return; }
  toast(t("editor.texts.no_owner"), "err");
}

function drawTextSide() {
  const side = $("side");
  side.replaceChildren();
  side.append(el("div", { class: "side-head" }, el("h2", {}, t("editor.texts"))));
  const seg = el("div", { class: "seg full" }, [["missing", t("editor.filter_missing")], ["all", t("editor.filter_all")]].map(([v, label]) =>
    el("button", { class: S.textFilter === v ? "on" : "", onclick: () => { S.textFilter = v; drawTextSide(); drawTexts(); } }, label)));
  const prefixes = [...new Set(S.texts.map((r) => r.key.split(".")[0]))].sort();
  const area = el("select", { class: "sm full", onchange: (e) => { S.textPrefix = e.target.value; drawTexts(); } },
    el("option", { value: "" }, t("editor.texts.all_areas")),
    prefixes.map((p) => el("option", { value: p, selected: p === S.textPrefix }, `${p} (${S.texts.filter((r) => r.key.startsWith(p + ".")).length})`)));
  side.append(seg, searchBox(S.filter, (v) => { S.filter = v; drawTexts(); }), area);
  const list = el("div", { class: "list" }, el("div", { class: "list-group" }, t("editor.locale")));
  const total = S.texts.length || 1;
  for (const l of S.meta.locales.filter((x) => x !== "en")) {
    const missing = S.texts.filter((r) => !r.texts[l]).length;
    const done = Math.round(100 * (total - missing) / total);
    list.append(el("button", { class: "entry" + (S.textLocale === l ? " sel" : ""), onclick: () => { S.textLocale = l; drawTextSide(); drawTexts(); } },
      el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(l)}` }, l.slice(0, 2).toUpperCase()),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, langName(l)),
        el("div", { class: "bar" }, el("span", { style: `width:${done}%` }))),
      missing ? el("span", { class: "badge warn" }, missing) : el("span", { class: "ok-mark" }, icon("check", 14))));
  }
  side.append(list);
}

function drawTexts() {
  const p = $("panel");
  p.replaceChildren();
  const loc = S.textLocale;
  const changes = {};
  const q = norm(S.filter);
  const rows = S.texts.filter((r) => (S.textFilter === "all" || !r.texts[loc] || r.review.includes(loc))
    && (!S.textPrefix || r.key.startsWith(S.textPrefix + "."))
    && (!q || norm(r.key).includes(q) || norm(r.texts.en).includes(q) || norm(r.texts[loc]).includes(q)));
  const page = el("div", { class: "page" });
  const save = el("button", { class: "primary", disabled: true, onclick: async () => {
    const body = { files: [], texts: Object.entries(changes).map(([key, text]) => ({ locale: loc, key, text })) };
    if (!body.texts.length) return;
    try {
      const r = await api("/api/save", body);
      if (r.ok) {
        toast(t("editor.saved"));
        await Promise.all([loadTexts(), reloadContent()]);
        drawRail(); drawTextSide(); drawTexts();
      } else {
        toast(t("editor.fix_first"), "err");
        page.prepend(el("div", { class: "alert err" }, el("ul", {}, r.report.errors.map((e) => el("li", {}, clip(e, r.report.parts))))));
      }
    } catch (e) { toast(e.message, "err"); }
  } }, icon("check", 16), t("editor.save"));
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, t("editor.texts"), el("code", {}, loc)), el("h1", {}, langName(loc))),
    el("div", { class: "grow" }), el("span", { class: "chip" }, `${rows.length} / ${S.texts.length}`), save));
  const list = el("div", { class: "tlist" });
  for (const r of rows) {
    const cur = r.texts[loc] ?? "";
    const max = r.max_chars;
    const counter = max ? el("span", { class: "counter" }, `${cur.length}/${max}`) : null;
    const inp = el("textarea", { rows: 1, maxlength: max, placeholder: langName(loc), oninput: (e) => {
      changes[r.key] = e.target.value;
      save.disabled = false;
      autosize(e.target);
      if (counter) counter.textContent = `${e.target.value.length}/${max}`;
    } }, cur);
    const review = r.review.includes(loc);
    list.append(el("div", { class: "trow" + (review ? " review" : "") },
      el("div", { class: "tsrc" }, el("div", { class: "en" }, r.texts.en ?? ""),
        el("button", { class: "tkey", title: t("editor.texts.open_owner"), onclick: () => openOwner(r.key) }, r.key),
        r.note ? el("div", { class: "tnote" }, r.note) : null),
      el("div", { class: "tdst" }, inp,
        el("div", { class: "tmeta" }, review ? el("span", { class: "chip warn" }, t("editor.review")) : null, el("div", { class: "grow" }), counter))));
  }
  if (!rows.length) {
    const allDone = S.textFilter !== "all" && !q && !S.textPrefix;
    list.append(el("div", { class: "empty-list" + (allDone ? " done" : "") }, allDone ? fmt("editor.texts.all_done", { lang: langName(loc) }) : t("editor.no_results")));
  }
  page.append(list);
  p.append(page);
  autosizeAll(list.querySelectorAll("textarea"));
}

window.addEventListener("beforeunload", (e) => { if (dirty()) e.preventDefault(); });
boot().catch((e) => toast(e.message, "err"));

// ---------- play and new game ----------

// Runs the server and the client on this game (`mmo play`). The button
// stays off while the game window is open.
async function playGame() {
  try {
    await api("/api/play", {});
  } catch (e) {
    await say(t("editor.launch.play_failed"), e.message);
    return;
  }
  S.playing = true;
  toast(t("editor.launch.play_started"));
  drawRail();
  watchPlay();
}

function watchPlay() {
  clearTimeout(S.playTimer);
  S.playTimer = setTimeout(async () => {
    const st = await api("/api/play").catch(() => ({ running: false }));
    if (st.running) return watchPlay();
    S.playing = false;
    drawRail();
  }, 2000);
}

// Copies the template beside this game and opens its editor in a new tab.
async function newGame() {
  const input = el("input", { class: "big mono", autofocus: true, placeholder: "my_game",
    oninput: () => { input.value = input.value.toLowerCase().replace(/[^a-z0-9_-]+/g, "_").slice(0, 40); } });
  const name = await dialog(t("editor.launch.new_game"), [
    el("p", { class: "hint" }, t("editor.launch.new_game_hint")),
    el("div", { class: "fld" }, el("label", {}, t("editor.launch.folder")), input),
  ], [
    { label: t("editor.cancel"), value: null },
    { label: t("editor.create"), value: "go", primary: true },
  ], { validate: () => !!input.value });
  if (!name) return;
  const tab = window.open("", "_blank");
  try {
    const r = await api("/api/new-game", { name: input.value });
    toast(t("editor.launch.created").replace("{folder}", r.folder));
    if (tab) tab.location = r.url; else window.location = r.url;
  } catch (e) {
    tab?.close();
    await say(t("editor.launch.new_game"), e.message);
  }
}
