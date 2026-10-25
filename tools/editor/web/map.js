// Map painter of the content editor.
// Edits zones/<id>/map.ron (tiles), zone.ron (origin, spawn, regions),
// npcs.ron, monsters.ron and nodes.ron together; the server checks and saves
// them as one change. Seen from above (not isometric): one square per tile,
// colored like the tile's sprite in assets/tiles.ron.
//
// Layout: tools on the left, the canvas in the middle, and on the right
// what the tool needs (tiles, what to place) plus the lists of what is on
// the map, the zone settings and where the zone sits in the world.
"use strict";

const M = {
  tool: "paint", ch: null, brush: 1, zoom: 12, grid: true, seam: true,
  npc: "", monster: "", node: "", count: 3, radius: 3, region: "Safe",
  tiles: null, colors: {}, down: false, drag: null, hover: null, sel: null,
  undo: [], redo: [], world: null, scroll: null, grid2: null, frame: 0, worldDrag: null, moving: null,
};
// Region kinds of the engine (defs::RegionKind).
const REGION_KINDS = ["Safe", "Pvp", "Workshop"];
const REGION_COLORS = { Safe: "#5fd38d", Pvp: "#ff5d5d", Workshop: "#f5b04c" };
// Width of the band by a neighbor zone that the other side sees
// no spawn, region or NPC.
const SEAM = 24;
// A monster spawn this big is dense: out of the seam band (defs::DENSE_SPAWN).
const DENSE_SPAWN = 6;
const UNDO_MAX = 200;
// Tiles of the neighbor zones drawn around the map while painting, so a
// gap in a wall can be lined up with the gap on the other side.
const PAD = 8;

// Zone files that go with a map.
const MAP_SIBLINGS = { zone: "zone.ron", npcs: "npcs.ron", monsters: "monsters.ron", nodes: "nodes.ron" };

// The tools, their key and a glyph. Labels and help are editor texts.
const TOOLS = [
  { id: "paint", key: "b", glyph: "✎" },
  { id: "fill", key: "g", glyph: "◪" },
  { id: "rect", key: "r", glyph: "▭" },
  { id: "line", key: "l", glyph: "╱" },
  { id: "pick", key: "i", glyph: "◉" },
  { id: "select", key: "v", glyph: "✥" },
  { id: "npc", key: "n", glyph: "☺" },
  { id: "monster", key: "m", glyph: "☠" },
  { id: "node", key: "o", glyph: "◆" },
  { id: "region", key: "a", glyph: "⬚" },
  { id: "spawn", key: "s", glyph: "★" },
];
const TILE_TOOLS = ["paint", "fill", "rect", "line"];

// Characters offered to a new tile of the legend, in order of preference.
const FREE_CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,#~_*=%+-^&$@!?:;<>/|'`\"()[]{}";

// ---------- load ----------

async function mapLoad(path) {
  await loadSiblings(path.replace(/map\.ron$/, ""), false);
  await mapReady();
}

async function loadSiblings(dir, isNew) {
  S.extra = {};
  for (const [k, file] of Object.entries(MAP_SIBLINGS)) {
    const f = await api("/api/file?path=" + encodeURIComponent(dir + file));
    S.extra[k] = f.json ?? (k === "zone" ? null : []);
    // A zone being created writes all of its files, even the empty ones.
    S.extra[k + "_existed"] = isNew || (f.json !== null && f.json !== undefined);
  }
}

async function mapReady() {
  if (!M.tiles) M.tiles = await api("/api/tiles");
  S.extra.nodes ||= [];
  const legend = Object.keys(S.value.legend || {});
  if (!legend.includes(M.ch)) M.ch = legend[0] ?? ".";
  M.npc ||= (S.content.ids.npc || [])[0] || "";
  M.monster ||= (S.content.ids.monster || [])[0] || "";
  M.node ||= (S.content.ids.gather_node || [])[0] || "";
  M.undo = []; M.redo = []; M.sel = null; M.scroll = null;
  await Promise.all([
    ...Object.values(S.value.legend || {}).map((t) => tileColor(t.floor)),
    loadWorld(),
  ]);
}

// Every other zone: where it sits and how big it is, for the world view and
// the seam bands. Read once per opened zone.
async function loadWorld() {
  const dir = S.sel.replace(/map\.ron$/, "");
  const zones = (S.content.files || []).filter((f) => f.kind === "map").map((f) => f.path.replace(/map\.ron$/, ""));
  const out = [];
  await Promise.all(zones.filter((z) => z !== dir).map(async (z) => {
    const [zf, mf, nf, sf] = await Promise.all(["zone.ron", "map.ron", "npcs.ron", "monsters.ron"]
      .map((f) => api("/api/file?path=" + encodeURIComponent(z + f))));
    if (!zf.json || !mf.json) return;
    const rows = (mf.json.rows || []).map((r) => Array.from(r));
    const legend = mf.json.legend || {};
    await Promise.all(Object.values(legend).map((d) => tileColor(d.floor)));
    out.push({ name: z.split("/").at(-2), path: z + "map.ron", x: zf.json.origin.x, y: zf.json.origin.y,
      w: rows[0]?.length ?? 0, h: rows.length, rows, legend,
      // What must stay out of a seam band (seamItems).
      zone: zf.json, npcs: nf.json || [], monsters: sf.json || [] });
  }));
  M.world = out.sort((a, b) => a.name.localeCompare(b.name));
  M.worldImg = null;
}

/// Side of the blank field a zone created from nothing starts with.
const NEW_ZONE_SIDE = 64;

// The extra fields of the "new zone" dialog: size and the first tile.
function mapNewOptions() {
  const w = el("input", { type: "number", min: 8, max: 256, value: NEW_ZONE_SIDE });
  const h = el("input", { type: "number", min: 8, max: 256, value: NEW_ZONE_SIDE });
  const floors = Object.keys(M.tiles || {}).map(Number);
  const base = el("select", {}, floors.map((f) => el("option", { value: f, selected: f === 1 }, tileName(f))));
  const box = el("div", { class: "fld" },
    el("label", {}, t("editor.map.new_size")),
    el("div", { class: "toolbar" }, w, el("span", {}, "×"), h, el("span", { class: "hint" }, t("editor.map.new_base")), base),
    el("div", { class: "hint" }, t("editor.map.new_hint")));
  return { box, get: () => ({ w: Math.max(8, Math.min(256, +w.value || NEW_ZONE_SIDE)),
    h: Math.max(8, Math.min(256, +h.value || NEW_ZONE_SIDE)), floor: +base.value || 1 }) };
}

// A zone is several files, so creating one creates all of them. It is put
// where no other zone is (east of the world), so it never starts out
// overlapping; the world view on the page moves it. A copy brings the
// spawn, the regions and the placed beings of the zone it came from,
// moved along with it.
async function mapNew(fromId, opts) {
  if (!M.tiles) M.tiles = await api("/api/tiles");
  // The other zones, before the new one exists.
  await loadWorld();
  if (fromId) {
    await loadSiblings(`zones/${fromId}/`, true);
    const rows = S.value.rows || [];
    const size = { w: Array.from(rows[0] || "").length, h: rows.length };
    const from = S.extra.zone?.origin ?? { x: 0, y: 0 };
    const to = freeSpot(size.w, size.h);
    shiftAll(to.x - from.x, to.y - from.y);
    if (S.extra.zone) { S.extra.zone.origin = to; S.extra.zone.starting = undefined; }
  } else {
    const o = opts || { w: NEW_ZONE_SIDE, h: NEW_ZONE_SIDE, floor: 1 };
    const ch = charFor(tileName(o.floor), new Set());
    S.value = { legend: { [ch]: { floor: o.floor } }, rows: Array.from({ length: o.h }, () => ch.repeat(o.w)) };
    const at = freeSpot(o.w, o.h);
    const mid = { x: at.x + Math.floor(o.w / 2), y: at.y + Math.floor(o.h / 2) };
    S.extra = {
      zone: { origin: at, view_radius: 20, spawn: mid, spawn_radius: 3,
        // A new zone is safe around its spawn: where a fallen newcomer returns.
        regions: [{ kind: "Safe", min: { x: mid.x - 6, y: mid.y - 6 }, max: { x: mid.x + 6, y: mid.y + 6 } }] },
      zone_existed: true,
      npcs: [], npcs_existed: true,
      monsters: [], monsters_existed: true,
      nodes: [], nodes_existed: false,
    };
  }
  await mapReady();
}

// A free place for a zone of w×h: right of everything, on the top row of
// the world, on a multiple of 32.
function freeSpot(w, h) {
  const zones = (M.world || []).filter((z) => z.name !== "load_test");
  if (!zones.length) return { x: 0, y: 0 };
  const right = Math.max(...zones.map((z) => z.x + z.w));
  const top = Math.min(...zones.map((z) => z.y));
  const x = Math.ceil((right + 32) / 32) * 32;
  let y = top;
  while ((M.world || []).some((z) => overlaps({ x, y, w, h }, z))) y += 32;
  return { x, y };
}

function overlaps(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

// Moves everything placed in global coordinates (spawn, regions, beings,
// nodes) by dx, dy: what moving the zone in the world needs.
function shiftAll(dx, dy) {
  if (!dx && !dy) return;
  const mv = (p) => { p.x += dx; p.y += dy; };
  const z = S.extra.zone;
  if (z) {
    mv(z.spawn);
    for (const r of z.regions || []) { mv(r.min); mv(r.max); }
  }
  for (const k of ["npcs", "monsters", "nodes"]) for (const s of S.extra[k] || []) mv(s.pos);
}

// Files the map save writes besides the map itself.
function mapExtraFiles() {
  const dir = S.sel.replace(/map\.ron$/, "");
  const out = [];
  if (S.extra.zone) out.push({ path: dir + "zone.ron", json: clean(S.extra.zone) });
  for (const k of ["npcs", "monsters", "nodes"]) {
    if ((S.extra[k] || []).length || S.extra[k + "_existed"]) out.push({ path: dir + MAP_SIBLINGS[k], json: clean(S.extra[k] || []) });
  }
  return out;
}

// ---------- tiles ----------

// Average color of a floor's sprite, or a stable color from its number.
async function tileColor(floor) {
  if (M.colors[floor]) return M.colors[floor];
  const look = M.tiles?.[floor];
  let color = `hsl(${(floor * 67) % 360} 35% 40%)`;
  if (look?.sprite) {
    try {
      const img = new Image();
      img.src = artUrl("sprite", look.sprite);
      await img.decode();
      const c = document.createElement("canvas");
      c.width = img.width; c.height = img.height;
      const g = c.getContext("2d");
      g.drawImage(img, 0, 0);
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let r = 0, gr = 0, b = 0, n = 0;
      for (let i = 0; i < d.length; i += 4) if (d[i + 3] > 128) { r += d[i]; gr += d[i + 1]; b += d[i + 2]; n++; }
      if (n) color = `rgb(${r / n | 0},${gr / n | 0},${b / n | 0})`;
    } catch (_) { /* no image: keep the stable color */ }
  }
  M.colors[floor] = color;
  return color;
}

// What a floor is called on the page: its sprite name, readable.
function tileName(floor) {
  const s = M.tiles?.[floor]?.sprite;
  return s ? humanize(s) : `#${floor}`;
}

// A character for a new tile: the first letter of its name when free.
function charFor(name, taken) {
  const first = String(name || "").replace(/^#/, "").charAt(0).toLowerCase();
  for (const c of [first, first.toUpperCase(), ...FREE_CHARS]) if (c && !taken.has(c)) return c;
  return "?";
}

function legendCounts() {
  const n = {};
  for (const r of S.value.rows || []) for (const ch of r) n[ch] = (n[ch] || 0) + 1;
  return n;
}

// Adds or edits a tile of the legend: which floor, and what it blocks.
// What blocks is a game rule, so it lives here and not in tiles.ron; a
// floor drawn as a wall in tiles.ron starts blocking both.
async function editTile(ch) {
  const legend = S.value.legend ||= {};
  const old = ch ? legend[ch] : null;
  const floors = Object.keys(M.tiles || {}).map(Number);
  let floor = old?.floor ?? floors.find((f) => !Object.values(legend).some((d) => d.floor === f)) ?? floors[0] ?? 1;
  const move = el("input", { type: "checkbox", checked: !!old?.blocks_movement });
  // A player switches the tile to this floor by clicking it, and back
  // (a brazier lit or put out).
  const sw = el("select", {}, el("option", { value: "" }, t("editor.map.switch_none")),
    floors.map((f) => el("option", { value: f, selected: old?.switch === f }, `${f} · ${tileName(f)}`)));
  const sight = el("input", { type: "checkbox", checked: !!old?.blocks_sight });
  const grid = el("div", { class: "tile-grid" });
  const fill = () => {
    grid.replaceChildren(...floors.map((f) => el("button", {
      class: "tile-opt" + (f === floor ? " on" : ""), title: `floor ${f}`,
      onclick: () => {
        floor = f;
        if (!old) { const wall = !!M.tiles[f]?.wall; move.checked = wall; sight.checked = wall; }
        fill();
      },
    }, tileThumb(f), el("span", {}, tileName(f)))));
  };
  fill();
  const ok = await dialog(old ? `${t("editor.map.tile_edit")} '${ch}'` : t("editor.map.tile_new"), [
    el("div", { class: "hint" }, t("editor.map.tile_hint")),
    grid,
    el("label", { class: "check" }, move, t("editor.map.blocks_movement")),
    el("label", { class: "check" }, sight, t("editor.map.blocks_sight")),
    el("div", { class: "fld" }, el("label", {}, t("editor.map.switch")), sw,
      el("span", { class: "hint" }, t("editor.map.switch_hint"))),
  ], [
    { label: t("editor.cancel"), value: false },
    { label: old ? t("editor.ok") : t("editor.add"), value: true, primary: true },
  ], { wide: true });
  if (!ok) return;
  remember();
  const def = { floor };
  if (move.checked) def.blocks_movement = true;
  if (sight.checked) def.blocks_sight = true;
  if (sw.value && Number(sw.value) !== floor) def.switch = Number(sw.value);
  const key = ch ?? charFor(tileName(floor), new Set(Object.keys(legend)));
  legend[key] = def;
  M.ch = key;
  await tileColor(floor);
  changed(); redraw();
}

function removeTile(ch) {
  remember();
  delete S.value.legend[ch];
  M.ch = Object.keys(S.value.legend)[0] ?? null;
  changed(); redraw();
}

function tileThumb(floor) {
  const s = M.tiles?.[floor]?.sprite;
  const sw = el("span", { class: "tile-sw", style: `--c:${M.colors[floor] || "#444"}` });
  if (s && S.art) sw.append(frameThumb(s, 40));
  else if (s) sw.append(el("img", { src: artUrl("sprite", s), alt: "", loading: "lazy" }));
  return sw;
}

// ---------- undo ----------

function snapMap() {
  return JSON.stringify({ v: S.value, z: S.extra.zone, n: S.extra.npcs, m: S.extra.monsters, o: S.extra.nodes });
}

function restoreMap(s) {
  const o = JSON.parse(s);
  S.value = o.v; S.extra.zone = o.z; S.extra.npcs = o.n; S.extra.monsters = o.m; S.extra.nodes = o.o;
}

// Called before every change the user makes.
function remember() {
  M.undo.push(snapMap());
  if (M.undo.length > UNDO_MAX) M.undo.shift();
  M.redo = [];
}

function undoMap() {
  if (!M.undo.length) return;
  M.redo.push(snapMap());
  restoreMap(M.undo.pop());
  M.sel = null;
  changed(); redraw();
}

function redoMap() {
  if (!M.redo.length) return;
  M.undo.push(snapMap());
  restoreMap(M.redo.pop());
  M.sel = null;
  changed(); redraw();
}

// Keys of the map page: tools by letter, undo and redo, delete.
document.addEventListener("keydown", (e) => {
  if (S.view !== "content" || S.kind !== "map" || !S.sel || !S.extra || $("modal").childElementCount) return;
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName);
  const mod = e.ctrlKey || e.metaKey;
  const k = e.key.toLowerCase();
  if (mod && k === "z" && !typing) { e.preventDefault(); if (e.shiftKey) redoMap(); else undoMap(); return; }
  if (mod && k === "y" && !typing) { e.preventDefault(); redoMap(); return; }
  if (typing || mod || e.altKey) return;
  if ((e.key === "Delete" || e.key === "Backspace") && M.sel) { e.preventDefault(); deleteSel(); return; }
  if (e.key === "Escape" && M.sel) { M.sel = null; redraw(); return; }
  if (M.view !== "world" && (e.key === "+" || e.key === "=" || e.key === "-")) { setZoom(ZOOMS[Math.max(0, Math.min(ZOOMS.length - 1, zoomStep(M.zoom) + (e.key === "-" ? -1 : 1)))]); return; }
  if (M.view !== "world" && e.key === "0") { fitZoom(); return; }
  if (e.key === "\\") { toggleFocus(); return; }
  if (e.key === "[" || e.key === "]") { M.brush = Math.max(1, Math.min(9, M.brush + (e.key === "]" ? 2 : -2))); redraw(); return; }
  const tool = TOOLS.find((x) => x.key === k);
  if (tool) { M.tool = tool.id; M.drag = null; redraw(); }
});

// ---------- the page ----------

function mapRows() { return S.value.rows.map((r) => Array.from(r)); }
function origin() { return S.extra.zone?.origin ?? { x: 0, y: 0 }; }
function mapSize() { const r = S.value.rows || []; return { w: Array.from(r[0] || "").length, h: r.length }; }

// Focus: the file list hides and the rail shrinks, so the map gets the
// width. On by default; remembered in this browser.
function mapFocus() { return storeGet("editor.mapFocus") !== "0"; }
function toggleFocus() {
  storeSet("editor.mapFocus", mapFocus() ? "0" : "1");
  syncFocus();
  redraw();
}

function mapPanel() {
  M.zoom = Math.min(M.zoom, maxZoom());
  const out = el("div", {});
  const world = M.view === "world";
  // The two ways to look at a zone, always in sight: its tiles, or where
  // it sits among the others (how zones are joined).
  out.append(el("div", { class: "mapmodes" },
    el("button", { class: !world ? "on" : "", onclick: () => { M.view = "zone"; redraw(); } },
      el("span", { class: "glyph" }, "✎"), t("editor.map.mode_edit")),
    el("button", { class: world ? "on" : "", onclick: () => { M.view = "world"; redraw(); } },
      el("span", { class: "glyph" }, "◫"), t("editor.map.mode_world")),
    el("button", { class: "focusbtn", title: "\\", onclick: toggleFocus },
      el("span", { class: "glyph" }, mapFocus() ? "⇥" : "⇤"), t(mapFocus() ? "editor.map.panels_show" : "editor.map.panels_hide"))));
  out.append(el("div", { id: "joinbar" }, joinBanner()));
  if (world) {
    out.append(worldPage());
    setTimeout(paintBigWorld);
    return out;
  }
  const box = el("div", { class: "mapgrid" });
  box.append(toolColumn(), canvasColumn(), sideColumn());
  out.append(box);
  setTimeout(() => {
    const wrap = $("mapwrap");
    if (wrap && M.scroll) { wrap.scrollLeft = M.scroll.x; wrap.scrollTop = M.scroll.y; }
    paint();
  });
  return out;
}

// How the zone is joined to the world, in one line on top of the page:
// alone (nobody reaches it), overlapping (cannot be saved), or touching
// with or without a way through.
function joinBanner() {
  if (!S.extra?.zone || !M.world) return null;
  const go = M.view === "world" ? null
    : el("button", { class: "primary", onclick: () => { M.view = "world"; redraw(); } }, t("editor.map.go_world"));
  const o = origin(), { w, h } = mapSize();
  const clash = M.world.filter((z) => overlaps({ x: o.x, y: o.y, w, h }, z));
  if (clash.length) return el("div", { class: "joinbar err" }, el("b", {}, "⚠ "), fmt("editor.map.world_overlap", { zones: clash.map((z) => z.name).join(", ") }), go);
  const seam = seamClashes();
  if (seam.length) return el("div", { class: "joinbar err" },
    el("div", { class: "seamlist" }, seam.map((c) => el("div", {}, el("b", {}, "⚠ "),
      fmt("editor.map.seam_clash", { zone: c.zone, other: c.other, n: SEAM, items: c.items.join(", ") }))),
      el("div", { class: "hint" }, t("editor.map.seam_fix"))), go);
  const near = neighbors();
  // A game of one zone is fine: no bar. The warning is for a zone left apart from the others.
  if (!near.length && !M.world.length) return null;
  if (!near.length) return el("div", { class: "joinbar warn" }, el("b", {}, "⚠ "), t("editor.map.alone"), go);
  const ways = near.map((z) => ({ z, n: passages(z).length }));
  const closed = ways.filter((x) => !x.n);
  return el("div", { class: "joinbar" + (closed.length ? " warn" : " ok") },
    ways.map((x) => el("span", { class: "joinchip" + (x.n ? " ok" : " warn") },
      x.n ? fmt("editor.map.passages", { zone: x.z.name, n: x.n }) : fmt("editor.map.no_passage", { zone: x.z.name }))),
    closed.length ? el("span", { class: "hint" }, t("editor.map.open_hint")) : null, go);
}

function toolColumn() {
  const col = el("div", { class: "maptools" });
  for (const tl of TOOLS) {
    col.append(el("button", {
      class: "maptool" + (M.tool === tl.id ? " on" : ""),
      title: `${t("editor.map." + tl.id)} (${tl.key.toUpperCase()}) — ${t("editor.map.help." + tl.id)}`,
      onclick: () => { M.tool = tl.id; M.drag = null; redraw(); },
    }, el("span", { class: "glyph" }, tl.glyph), el("span", { class: "lbl" }, t("editor.map." + tl.id)), el("kbd", {}, tl.key.toUpperCase())));
  }
  col.append(el("div", { class: "sep" }),
    el("button", { class: "maptool", disabled: !M.undo.length, title: "Ctrl+Z", onclick: undoMap },
      el("span", { class: "glyph" }, "↶"), el("span", { class: "lbl" }, t("editor.map.undo"))),
    el("button", { class: "maptool", disabled: !M.redo.length, title: "Ctrl+Shift+Z", onclick: redoMap },
      el("span", { class: "glyph" }, "↷"), el("span", { class: "lbl" }, t("editor.map.redo"))));
  return col;
}

function canvasColumn() {
  const col = el("div", { class: "mapcenter" });
  // Line over the canvas: what the tool does, and the view.
  const zoom = el("input", { type: "range", id: "mapzoom", min: 0, max: ZOOMS.length - 1, value: zoomStep(M.zoom),
    title: t("editor.map.zoom_hint"), oninput: (e) => setZoom(ZOOMS[+e.target.value]) });
  col.append(el("div", { class: "toolbar mapbar" },
    el("span", { class: "tool-help" }, el("b", {}, t("editor.map." + M.tool)), " · ", t("editor.map.help." + M.tool)),
    el("div", { class: "grow" }),
    el("label", { class: "check" }, el("input", { type: "checkbox", checked: M.grid, onchange: (e) => { M.grid = e.target.checked; paint(); } }), t("editor.map.grid")),
    el("label", { class: "check" }, el("input", { type: "checkbox", checked: M.seam, onchange: (e) => { M.seam = e.target.checked; paint(); } }), t("editor.map.seam")),
    el("label", { class: "hint" }, t("editor.map.zoom")), zoom,
    el("span", { class: "hint zoomval", id: "zoomval" }, `${M.zoom}px`),
    el("button", { class: "sm", title: "0", onclick: fitZoom }, t("editor.map.fit"))));
  const canvas = el("canvas", { id: "map", oncontextmenu: (e) => e.preventDefault() });
  canvas.addEventListener("mousedown", onDown);
  canvas.addEventListener("mousemove", onMove);
  canvas.addEventListener("mouseleave", () => { M.hover = null; paint(); });
  const wrap = el("div", { class: "mapwrap", id: "mapwrap", onscroll: (e) => { M.scroll = { x: e.target.scrollLeft, y: e.target.scrollTop }; } }, canvas);
  // On the whole box, not just the canvas: Ctrl + wheel over the dark
  // margin must not zoom the browser page instead.
  wrap.addEventListener("wheel", onWheel, { passive: false });
  col.append(wrap, el("div", { id: "mapinfo", class: "hint" }, t("editor.map.hover_hint")));
  window.onmouseup = onUp;
  return col;
}

function sideColumn() {
  const col = el("div", { class: "mapside" });
  col.append(toolOptions());
  col.append(objectLists());
  col.append(zoneCard());
  return col;
}

function sideCard(title, ...kids) {
  return el("section", { class: "mapcard" }, el("h4", {}, title), ...kids);
}

// What the current tool needs: the tiles for the tile tools, the content
// to place for the others.
function toolOptions() {
  if (TILE_TOOLS.includes(M.tool) || M.tool === "pick") return tilePalette();
  const tip = (k) => el("p", { class: "hint" }, t(k));
  if (M.tool === "npc") return sideCard(t("editor.map.npc"), refPick("npc", M.npc, (v) => { M.npc = v; }), tip("editor.map.help.npc"));
  if (M.tool === "monster") {
    return sideCard(t("editor.map.monster"), refPick("monster", M.monster, (v) => { M.monster = v; }),
      el("div", { class: "row2" },
        el("label", {}, t("editor.map.count"), el("input", { type: "number", min: 1, value: M.count, oninput: (e) => { M.count = Math.max(1, +e.target.value || 1); } })),
        el("label", {}, t("editor.map.radius"), el("input", { type: "number", min: 0, value: M.radius, oninput: (e) => { M.radius = Math.max(0, +e.target.value || 0); } }))),
      tip("editor.map.monster_hint"));
  }
  if (M.tool === "node") return sideCard(t("editor.map.node"), refPick("gather_node", M.node, (v) => { M.node = v; }), tip("editor.map.help.node"));
  if (M.tool === "region") {
    return sideCard(t("editor.map.region"),
      el("div", { class: "seg full" }, REGION_KINDS.map((k) => el("button", { class: M.region === k ? "on" : "",
        style: `border-bottom: 2px solid ${REGION_COLORS[k]}`, onclick: () => { M.region = k; redraw(); } }, vlabel(k)))),
      tip("editor.map.region_" + M.region.toLowerCase()));
  }
  if (M.tool === "spawn" && S.extra.zone) {
    return sideCard(t("editor.map.spawn"),
      el("label", {}, t("editor.map.spawn_radius"), el("input", { type: "number", min: 0, value: S.extra.zone.spawn_radius ?? 0,
        onchange: (e) => { remember(); S.extra.zone.spawn_radius = Math.max(0, +e.target.value || 0); changed(); paint(); } })),
      tip("editor.map.help.spawn"));
  }
  return sideCard(t("editor.map.select"), tip("editor.map.select_hint"));
}

function refPick(kind, value, set) {
  const ids = S.content.ids[kind] || [];
  if (!ids.length) return el("p", { class: "hint" }, t("editor.map.none_yet"));
  return el("select", { onchange: (e) => set(e.target.value) },
    ids.map((id) => el("option", { value: id, selected: id === value }, `${refTitle(kind, id)} (${id})`)));
}

function tilePalette() {
  const legend = S.value.legend || {};
  const counts = legendCounts();
  const list = el("div", { class: "tile-list" });
  for (const [ch, def] of Object.entries(legend)) {
    const flags = [];
    if (def.blocks_movement) flags.push(el("span", { class: "tag warn" }, t("editor.map.tag_blocks")));
    if (def.blocks_sight) flags.push(el("span", { class: "tag" }, t("editor.map.tag_sight")));
    list.append(el("div", { class: "tile-row" + (M.ch === ch ? " on" : ""), onclick: () => { M.ch = ch; if (M.tool === "pick") M.tool = "paint"; redraw(); } },
      tileThumb(def.floor),
      el("div", { class: "tile-txt" }, el("b", {}, tileName(def.floor)),
        el("span", { class: "hint" }, `'${ch}' · ${counts[ch] || 0} ${t("editor.map.tiles_n")}`), el("div", {}, ...flags)),
      el("button", { class: "icon-btn", title: t("editor.map.tile_edit"), onclick: (e) => { e.stopPropagation(); editTile(ch); } }, "✎"),
      counts[ch] ? null : el("button", { class: "icon-btn", title: t("editor.remove"), onclick: (e) => { e.stopPropagation(); removeTile(ch); } }, "×")));
  }
  const brush = el("div", { class: "row2" });
  if (M.tool === "paint") {
    brush.append(el("label", {}, t("editor.map.brush"),
      el("select", { onchange: (e) => { M.brush = +e.target.value; } }, [1, 3, 5, 7, 9].map((n) => el("option", { value: n, selected: M.brush === n }, `${n}×${n}`)))));
  }
  return sideCard(t("editor.map.legend"), list,
    el("button", { class: "ghost wide", onclick: () => editTile(null) }, "+ ", t("editor.map.tile_new")),
    brush, el("p", { class: "hint" }, t("editor.map.legend_hint")));
}

// What is on the map, as lists: click to find, edit numbers, remove.
function objectLists() {
  const box = el("div", {});
  const row = (k, i, label, sub, extra) => el("div", {
    class: "obj-row" + (M.sel?.k === k && M.sel?.i === i ? " on" : ""),
    onclick: () => { M.sel = { k, i }; focusSel(); redraw(); },
  }, el("div", { class: "obj-txt" }, el("b", {}, label), el("span", { class: "hint" }, sub)), extra,
  el("button", { class: "icon-btn", title: t("editor.remove"), onclick: (e) => { e.stopPropagation(); M.sel = { k, i }; deleteSel(); } }, "×"));
  const at = (p) => `(${p.x}, ${p.y})`;
  const num = (v, set) => el("input", { type: "number", min: 0, value: v, class: "mini",
    onclick: (e) => e.stopPropagation(), onchange: (e) => { remember(); set(Math.max(0, +e.target.value || 0)); changed(); paint(); } });
  const regions = S.extra.zone?.regions || [];
  const sec = (title, n, kids) => el("details", { class: "mapcard", open: n > 0 && n < 40 },
    el("summary", {}, `${title} (${n})`), ...kids);
  box.append(sec(t("editor.map.list_regions"), regions.length, regions.map((r, i) => row("regions", i,
    el("span", { style: `color:${REGION_COLORS[r.kind]}` }, vlabel(r.kind)), `${at(r.min)} – ${at(r.max)}`,
    el("select", { class: "mini", onclick: (e) => e.stopPropagation(),
      onchange: (e) => { remember(); r.kind = e.target.value; changed(); redraw(); } },
    REGION_KINDS.map((k) => el("option", { value: k, selected: k === r.kind }, vlabel(k))))))));
  box.append(sec(t("editor.map.list_npcs"), S.extra.npcs.length, S.extra.npcs.map((n, i) => row("npcs", i, refTitle("npc", n.npc), at(n.pos)))));
  box.append(sec(t("editor.map.list_monsters"), S.extra.monsters.length, S.extra.monsters.map((m, i) => row("monsters", i,
    refTitle("monster", m.monster), at(m.pos),
    el("span", { class: "obj-nums" }, "×", num(m.count ?? 1, (v) => { m.count = Math.max(1, v); }), "r", num(m.radius ?? 0, (v) => { m.radius = v; }))))));
  box.append(sec(t("editor.map.list_nodes"), S.extra.nodes.length, S.extra.nodes.map((n, i) => row("nodes", i, refTitle("gather_node", n.node), at(n.pos)))));
  return box;
}

// The zone itself: where it is, how far players see, its size, and the
// world around it.
function zoneCard() {
  const z = S.extra.zone;
  if (!z) return el("div", {});
  const { w, h } = mapSize();
  const num = (v, set, min) => el("input", { type: "number", value: v, min,
    onchange: (e) => { remember(); set(+e.target.value || 0); changed(); redraw(); } });
  const W = el("input", { type: "number", min: 1, max: 256, value: w });
  const H = el("input", { type: "number", min: 1, max: 256, value: h });
  const anchor = el("select", {},
    el("option", { value: "end" }, t("editor.map.grow_end")),
    el("option", { value: "start" }, t("editor.map.grow_start")));
  return el("div", {},
    sideCard(t("editor.map.zone"),
      el("label", {}, t("editor.map.view_radius"), num(z.view_radius ?? 20, (v) => { z.view_radius = Math.max(1, v); }, 1)),
      el("label", { class: "check" }, el("input", { type: "checkbox", checked: !!z.starting,
        onchange: (e) => { remember(); z.starting = e.target.checked || undefined; changed(); } }), t("editor.map.starting")),
      el("p", { class: "hint" }, t("editor.map.starting_hint")),
      el("label", {}, t("editor.map.size")),
      el("div", { class: "row2" }, W, el("span", {}, "×"), H),
      anchor,
      el("button", { class: "wide", onclick: () => { remember(); resize(+W.value, +H.value, anchor.value); changed(); redraw(); } }, t("editor.map.resize")),
      el("p", { class: "hint" }, t("editor.map.resize_hint"))),
    sideCard(t("editor.map.world"),
      el("div", { class: "row2" },
        el("label", {}, "X", num(z.origin.x, (v) => { moveZone(v - z.origin.x, 0); })),
        el("label", {}, "Y", num(z.origin.y, (v) => { moveZone(0, v - z.origin.y); }))),
      el("button", { class: "wide", onclick: () => { M.view = "world"; redraw(); } }, "◫ ", t("editor.map.mode_world"))));
}

function moveZone(dx, dy) {
  if (!dx && !dy) return;
  S.extra.zone.origin = { x: S.extra.zone.origin.x + dx, y: S.extra.zone.origin.y + dy };
  shiftAll(dx, dy);
}

// ---------- the world view ----------
//
// Every zone drawn with its real tiles, one pixel per tile, scaled. The
// open zone is in blue and is dragged to move it; near an edge of another
// zone it snaps to it. Where both sides of a shared edge can be walked,
// the edge is a passage and is drawn green.

const SNAP = 6;

function worldPage() {
  const z = S.extra.zone;
  const near = neighbors();
  const side = el("div", { class: "mapside" },
    sideCard(t("editor.map.world"),
      el("p", { class: "hint" }, t("editor.map.world_hint")),
      el("p", { class: "hint" }, t("editor.map.snap_hint")),
      el("div", { class: "row2" },
        el("label", {}, "X", el("input", { type: "number", value: z.origin.x, onchange: (e) => { remember(); moveZone(+e.target.value - z.origin.x, 0); changed(); redraw(); } })),
        el("label", {}, "Y", el("input", { type: "number", value: z.origin.y, onchange: (e) => { remember(); moveZone(0, +e.target.value - z.origin.y); changed(); redraw(); } })))),
    sideCard(t("editor.map.neighbors"),
      near.length ? near.map((n) => {
        const ways = passages(n);
        return el("div", { class: "obj-row", onclick: () => open(n.path) },
          el("div", { class: "obj-txt" }, el("b", {}, n.name),
            el("span", { class: ways.length ? "hint" : "err-text" },
              ways.length ? fmt("editor.map.passages", { zone: n.name, n: ways.length }) : fmt("editor.map.no_passage", { zone: n.name }))),
          el("span", { class: "hint" }, "→"));
      }) : M.world.length ? el("p", { class: "err-text" }, t("editor.map.alone")) : null),
    sideCard(t("editor.map.legend_world"),
      el("div", { class: "legend-row" }, el("i", { style: "background:#3fb8ff" }), t("editor.map.lw_this")),
      el("div", { class: "legend-row" }, el("i", { style: "background:#5fd38d" }), t("editor.map.lw_passage")),
      el("div", { class: "legend-row" }, el("i", { style: "background:#ff5d5d" }), t("editor.map.lw_overlap")),
      el("div", { class: "legend-row" }, el("i", { style: "background:transparent;border:2px solid #ff5d5d" }), t("editor.map.lw_seam"))));
  const canvas = el("canvas", { id: "bigworld" });
  canvas.addEventListener("mousedown", worldDown);
  canvas.addEventListener("mousemove", worldMove);
  return el("div", { class: "worldgrid" }, el("div", { class: "worldwrap" }, canvas, el("div", { id: "worldinfo", class: "hint" })), side);
}

function worldBox(c) {
  const { w, h } = mapSize();
  const o = origin();
  const me = { name: selId(), x: o.x, y: o.y, w, h, me: true };
  // load_test sits far away and would shrink the view; it shows only when in the way.
  const all = [...(M.world || []).filter((z) => z.name !== "load_test" || overlaps(z, me)), me];
  const pad = 24;
  const x0 = Math.min(...all.map((z) => z.x)) - pad, y0 = Math.min(...all.map((z) => z.y)) - pad;
  const x1 = Math.max(...all.map((z) => z.x + z.w)) + pad, y1 = Math.max(...all.map((z) => z.y + z.h)) + pad;
  const k = Math.max(0.5, Math.min(8, Math.min(((c.parentElement?.clientWidth || 900) - 4) / (x1 - x0), 720 / (y1 - y0))));
  return { all, x0, y0, x1, y1, k };
}

// A zone's tiles as an image, one pixel per tile.
function zoneImage(rows, legend) {
  const h = rows.length, w = rows[0]?.length ?? 0;
  const c = document.createElement("canvas");
  c.width = Math.max(1, w); c.height = Math.max(1, h);
  const g = c.getContext("2d");
  rows.forEach((r, y) => r.forEach((ch, x) => {
    const d = legend[ch];
    g.fillStyle = d ? (M.colors[d.floor] || "#444") : "#f0f";
    g.fillRect(x, y, 1, 1);
    if (d?.blocks_movement) { g.fillStyle = "rgba(0,0,0,.35)"; g.fillRect(x, y, 1, 1); }
  }));
  return c;
}

function paintBigWorld() {
  const c = $("bigworld");
  if (!c || !S.extra?.zone) return;
  const { all, x0, y0, x1, y1, k } = worldBox(c);
  c.width = Math.round((x1 - x0) * k); c.height = Math.round((y1 - y0) * k);
  const g = c.getContext("2d");
  g.imageSmoothingEnabled = false;
  g.fillStyle = "#07090c"; g.fillRect(0, 0, c.width, c.height);
  // A faint grid of 32 tiles helps line zones up.
  g.strokeStyle = "rgba(255,255,255,.04)";
  for (let x = Math.ceil(x0 / 32) * 32; x < x1; x += 32) { g.beginPath(); g.moveTo((x - x0) * k + .5, 0); g.lineTo((x - x0) * k + .5, c.height); g.stroke(); }
  for (let y = Math.ceil(y0 / 32) * 32; y < y1; y += 32) { g.beginPath(); g.moveTo(0, (y - y0) * k + .5); g.lineTo(c.width, (y - y0) * k + .5); g.stroke(); }
  const me = all.find((z) => z.me);
  const clash = all.filter((z) => !z.me && overlaps(z, me));
  for (const z of all) {
    const img = z.me ? zoneImage(M.grid2 ?? mapRows(), S.value.legend || {}) : (z.img ||= zoneImage(z.rows, z.legend));
    g.globalAlpha = z.me ? 1 : .75;
    g.drawImage(img, (z.x - x0) * k, (z.y - y0) * k, z.w * k, z.h * k);
    g.globalAlpha = 1;
    const bad = z.me ? clash.length > 0 : clash.includes(z);
    g.strokeStyle = bad ? "#ff5d5d" : z.me ? "#3fb8ff" : "rgba(255,255,255,.35)";
    g.lineWidth = z.me ? 3 : 1;
    g.strokeRect((z.x - x0) * k + .5, (z.y - y0) * k + .5, z.w * k - 1, z.h * k - 1);
    // Name in a tag at the top left.
    g.font = (z.me ? "bold " : "") + "12px system-ui";
    const tw = g.measureText(z.name).width;
    g.fillStyle = z.me ? "#3fb8ff" : "rgba(0,0,0,.7)";
    g.fillRect((z.x - x0) * k + 2, (z.y - y0) * k + 2, tw + 8, 16);
    g.fillStyle = z.me ? "#00131f" : "#e8ecf2";
    g.fillText(z.name, (z.x - x0) * k + 6, (z.y - y0) * k + 14);
  }
  // Passages: green marks on the shared edge, where both sides are open.
  for (const n of neighbors()) {
    for (const p of passages(n)) {
      g.fillStyle = "#5fd38d";
      const px = (p.x - x0) * k, py = (p.y - y0) * k;
      if (p.vertical) g.fillRect(px - k, py, Math.max(2 * k, 4), k);
      else g.fillRect(px, py - k, k, Math.max(2 * k, 4));
    }
  }
  // What breaks the seam rule, on either side: red boxes.
  g.strokeStyle = "#ff5d5d"; g.lineWidth = 2;
  for (const c of seamClashes()) for (const r of c.rects) g.strokeRect((r.x - x0) * k, (r.y - y0) * k, Math.max(r.w * k, 4), Math.max(r.h * k, 4));
  g.lineWidth = 1;
  const info = $("worldinfo");
  if (info) {
    const o = origin();
    info.textContent = `${selId()}: (${o.x}, ${o.y}) · ${me.w}×${me.h}` + (clash.length ? " · " + fmt("editor.map.world_overlap", { zones: clash.map((z) => z.name).join(", ") }) : "");
    info.classList.toggle("err-text", clash.length > 0);
  }
}

function worldDown(e) {
  const c = e.currentTarget;
  const b = worldBox(c);
  const r = c.getBoundingClientRect();
  const sx = c.width / r.width;
  const wx = (e.clientX - r.left) * sx / b.k + b.x0, wy = (e.clientY - r.top) * sx / b.k + b.y0;
  const o = origin(), { w, h } = mapSize();
  if (wx < o.x || wy < o.y || wx > o.x + w || wy > o.y + h) return;
  remember();
  M.worldDrag = { wx, wy, ox: o.x, oy: o.y, box: b, sx };
  c.style.cursor = "grabbing";
  window.onmouseup = () => {
    if (!M.worldDrag) return;
    M.worldDrag = null;
    const o2 = origin();
    if (o2.x === o.x && o2.y === o.y) M.undo.pop(); else changed();
    redraw();
  };
}

function worldMove(e) {
  const d = M.worldDrag;
  if (!d) return;
  const r = e.currentTarget.getBoundingClientRect();
  const wx = (e.clientX - r.left) * d.sx / d.box.k + d.box.x0, wy = (e.clientY - r.top) * d.sx / d.box.k + d.box.y0;
  const { x, y } = snapTo(Math.round(d.ox + wx - d.wx), Math.round(d.oy + wy - d.wy));
  const o = origin();
  if (x === o.x && y === o.y) return;
  moveZone(x - o.x, y - o.y);
  paintBigWorld();
  // The banner on top follows the drag (overlap, neighbors, passages).
  $("joinbar")?.replaceChildren(...[joinBanner()].filter(Boolean));
}

// Near an edge of another zone, the moved zone sticks to it (side by side,
// and lined up with its corner when close to it); elsewhere it moves in
// steps of 8. A place is only snapped to when the zone really touches that
// neighbor there and overlaps no zone: snapping never makes an overlap.
function snapTo(x, y) {
  const { w, h } = mapSize();
  const world = M.world || [];
  const free = (p) => !world.some((z) => overlaps({ x: p.x, y: p.y, w, h }, z));
  // Along the shared edge: lined up with either corner when near it, else where the mouse is.
  const along = (v, lo, len, zlo, zlen) => {
    for (const c of [zlo, zlo + zlen - len]) if (Math.abs(c - v) <= SNAP) return c;
    return v;
  };
  let best = null;
  for (const z of world) {
    const cands = [];
    for (const cx of [z.x + z.w, z.x - w]) cands.push({ x: cx, y: along(y, y, h, z.y, z.h) });
    for (const cy of [z.y + z.h, z.y - h]) cands.push({ x: along(x, x, w, z.x, z.w), y: cy });
    for (const p of cands) {
      const d = Math.max(Math.abs(p.x - x), Math.abs(p.y - y));
      if (d > SNAP) continue;
      // Shares a stretch of edge (not just a corner) with z, and overlaps nothing.
      const shares = p.x < z.x + z.w && z.x < p.x + w || p.y < z.y + z.h && z.y < p.y + h;
      if (!shares || !free(p)) continue;
      if (!best || d < best.d) best = { ...p, d };
    }
  }
  return best ? { x: best.x, y: best.y } : { x: Math.round(x / 8) * 8, y: Math.round(y / 8) * 8 };
}

// Can a tile at a global position be walked? Looks in this zone, then the others.
function walkableAt(gx, gy) {
  const o = origin();
  const rows = M.grid2 ?? mapRows();
  const lx = gx - o.x, ly = gy - o.y;
  if (rows[ly]?.[lx] !== undefined) { const d = (S.value.legend || {})[rows[ly][lx]]; return !!d && !d.blocks_movement; }
  for (const z of M.world || []) {
    const ch = z.rows[gy - z.y]?.[gx - z.x];
    if (ch !== undefined) { const d = z.legend[ch]; return !!d && !d.blocks_movement; }
  }
  return false;
}

// The tiles of this zone's edge with a neighbor where a player can cross:
// open on this side and on the other. `vertical`: the shared edge is a
// vertical line (the zones are side by side).
function passages(n) {
  const o = origin(), { w, h } = mapSize();
  const out = [];
  const span = (a0, a1, b0, b1) => [Math.max(a0, b0), Math.min(a1, b1)];
  if (n.x + n.w === o.x || o.x + w === n.x) {
    const left = n.x + n.w === o.x;
    const mx = left ? o.x : o.x + w - 1, nx = left ? o.x - 1 : o.x + w;
    const [a, b] = span(o.y, o.y + h, n.y, n.y + n.h);
    for (let y = a; y < b; y++) if (walkableAt(mx, y) && walkableAt(nx, y)) out.push({ x: left ? o.x : o.x + w, y, vertical: true });
  }
  if (n.y + n.h === o.y || o.y + h === n.y) {
    const top = n.y + n.h === o.y;
    const my = top ? o.y : o.y + h - 1, ny = top ? o.y - 1 : o.y + h;
    const [a, b] = span(o.x, o.x + w, n.x, n.x + n.w);
    for (let x = a; x < b; x++) if (walkableAt(x, my) && walkableAt(x, ny)) out.push({ x, y: top ? o.y : o.y + h, vertical: false });
  }
  return out;
}

// Zones that touch this one by a side or a corner: their seam band.
function neighbors() {
  const o = origin(), { w, h } = mapSize();
  const me = { x: o.x, y: o.y, w, h };
  const grow = { x: me.x - 1, y: me.y - 1, w: me.w + 2, h: me.h + 2 };
  return (M.world || []).filter((z) => overlaps(grow, z) && !overlaps(me, z));
}

// The band (local tiles) by each neighbor: this zone's tiles within SEAM
// tiles of it, as the loader counts (only along the stretch it touches).
function seamBands() {
  const o = origin(), me = myRect();
  const out = [];
  for (const z of neighbors()) {
    const g = grown(z, SEAM);
    const x = Math.max(g.x, me.x), y = Math.max(g.y, me.y);
    const x1 = Math.min(g.x + g.w, me.x + me.w), y1 = Math.min(g.y + g.h, me.y + me.h);
    if (x < x1 && y < y1) out.push({ x: x - o.x, y: y - o.y, w: x1 - x, h: y1 - y });
  }
  return out;
}

// Same rule as the loader (defs::zone::world_problems): a thing is in the
// seam band when it comes within SEAM tiles (Chebyshev) of a neighbor.
function inSeam(p) {
  const me = myRect();
  return neighbors().some((n) => rectGap(box(p, p), n) <= SEAM && rectGap(box(p, p), me) === 0);
}

function myRect() { const o = origin(), { w, h } = mapSize(); return { x: o.x, y: o.y, w, h }; }
// A rectangle {x, y, w, h} from two corners, both inside.
function box(a, b) { return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x) + 1, h: Math.abs(b.y - a.y) + 1 }; }
function grown(r, n) { return { x: r.x - n, y: r.y - n, w: r.w + 2 * n, h: r.h + 2 * n }; }
// Tiles between two rectangles (0: they overlap, 1: they touch).
function rectGap(a, b) {
  const dx = Math.max(b.x - (a.x + a.w - 1), a.x - (b.x + b.w - 1), 0);
  const dy = Math.max(b.y - (a.y + a.h - 1), a.y - (b.y + b.h - 1), 0);
  return Math.max(dx, dy);
}

// What a zone has that must stay out of the seam band: the spawn with its
// radius, the regions, the NPCs and the dense monster spawns.
function seamItems(zone, npcs, monsters) {
  const out = [];
  if (zone?.spawn) out.push({ what: t("editor.map.seam_spawn"), r: grown(box(zone.spawn, zone.spawn), zone.spawn_radius ?? 0) });
  for (const r of zone?.regions || []) out.push({ what: fmt("editor.map.seam_region", { kind: vlabel(r.kind) }), r: box(r.min, r.max) });
  for (const n of npcs || []) out.push({ what: fmt("editor.map.seam_npc", { npc: refTitle("npc", n.npc) || n.npc }), r: box(n.pos, n.pos) });
  for (const m of monsters || []) {
    if ((m.count ?? 0) >= DENSE_SPAWN) out.push({ what: fmt("editor.map.seam_dense", { monster: refTitle("monster", m.monster) || m.monster }), r: grown(box(m.pos, m.pos), m.radius ?? 0) });
  }
  return out;
}

// Where this zone, put where it is, breaks the seam rule: its own things
// near a neighbor, and the neighbors' things near it (a city on the other
// side). The loader refuses to save either. [{ zone, other, items, rects }]
function seamClashes() {
  const me = { ...myRect(), name: selId() };
  const mine = seamItems(S.extra.zone, S.extra.npcs, S.extra.monsters);
  const out = [];
  for (const n of neighbors()) {
    for (const [zone, other, list] of [[me, n, mine], [n, me, seamItems(n.zone, n.npcs, n.monsters)]]) {
      const bad = list.filter((it) => rectGap(it.r, other) <= SEAM);
      if (bad.length) out.push({ zone: zone.name, other: other.name, items: bad.map((b) => b.what), rects: bad.map((b) => b.r) });
    }
  }
  return out;
}

// ---------- zoom ----------
//
// Pixels per tile, in fixed steps (one wheel notch = one step). The tile
// under the mouse (or the middle of the view) stays where it is.
const ZOOMS = [2, 3, 4, 5, 6, 8, 10, 12, 14, 16, 20, 24, 28, 32];
// The canvas stays under what browsers can draw (16384 px a side).
const CANVAS_MAX = 16000;

function zoomStep(z) {
  let best = 0;
  ZOOMS.forEach((v, i) => { if (Math.abs(v - z) < Math.abs(ZOOMS[best] - z)) best = i; });
  return best;
}

function maxZoom() {
  const { w, h } = mapSize();
  const side = Math.max(w, h) + 2 * PAD;
  return ZOOMS.filter((v) => v * side <= CANVAS_MAX).at(-1) ?? ZOOMS[0];
}

// Zooms to z keeping the point (ax, ay) of the box (default: its middle) still.
function setZoom(z, ax, ay) {
  const wrap = $("mapwrap");
  z = Math.min(ZOOMS[zoomStep(z)], maxZoom());
  if (!wrap || z === M.zoom) return;
  ax ??= wrap.clientWidth / 2; ay ??= wrap.clientHeight / 2;
  // In tiles (fractional), the point under the anchor before zooming. A
  // canvas smaller than the box is centered in it (offsetLeft/Top).
  const c = $("map");
  const tx = (wrap.scrollLeft + ax - c.offsetLeft) / M.zoom, ty = (wrap.scrollTop + ay - c.offsetTop) / M.zoom;
  M.zoom = z;
  paint();
  wrap.scrollLeft = tx * z + c.offsetLeft - ax;
  wrap.scrollTop = ty * z + c.offsetTop - ay;
  M.scroll = { x: wrap.scrollLeft, y: wrap.scrollTop };
  const s = $("mapzoom"); if (s) s.value = zoomStep(z);
  const v = $("zoomval"); if (v) v.textContent = `${z}px`;
}

// Trackpads send many small wheel events: they add up to one step.
function onWheel(e) {
  if (!e.ctrlKey && !e.metaKey) return;
  e.preventDefault();
  M.wheel = (M.wheel || 0) + (e.deltaMode === 1 ? e.deltaY * 40 : e.deltaY);
  if (Math.abs(M.wheel) < 50) return;
  const dir = M.wheel < 0 ? 1 : -1;
  M.wheel = 0;
  const wrap = $("mapwrap"), r = wrap.getBoundingClientRect();
  const i = Math.max(0, Math.min(ZOOMS.length - 1, zoomStep(M.zoom) + dir));
  setZoom(ZOOMS[i], e.clientX - r.left - wrap.clientLeft, e.clientY - r.top - wrap.clientTop);
}

// The whole zone (and the margin around it) in sight.
function fitZoom() {
  const wrap = $("mapwrap");
  if (!wrap) return;
  const { w, h } = mapSize();
  const k = Math.min(wrap.clientWidth / (w + 2 * PAD), wrap.clientHeight / (h + 2 * PAD));
  const z = ZOOMS.filter((v) => v <= k).at(-1) ?? ZOOMS[0];
  setZoom(z);
  wrap.scrollLeft = 0; wrap.scrollTop = 0;
  M.scroll = { x: 0, y: 0 };
}

// ---------- mouse ----------

function cell(e) {
  const r = $("map").getBoundingClientRect();
  return { x: Math.floor((e.clientX - r.left) / M.zoom) - PAD, y: Math.floor((e.clientY - r.top) / M.zoom) - PAD };
}

function global(c) { const o = origin(); return { x: c.x + o.x, y: c.y + o.y }; }

function onDown(e) {
  const c = cell(e);
  M.down = true;
  M.start = c; M.last = c;
  // Right button, any tool: what is placed under the cursor goes away.
  if (e.button === 2) { eraseAt(c); M.down = false; return; }
  if (e.button !== 0) return;
  const tool = M.tool;
  if (tool === "pick") {
    const ch = Array.from(S.value.rows[c.y] || "")[c.x];
    if (ch !== undefined) { M.ch = ch; M.tool = "paint"; redraw(); }
    M.down = false;
    return;
  }
  if (tool === "select") { startSelect(c); return; }
  remember();
  const pos = global(c);
  if (tool === "paint") { M.grid2 = mapRows(); stamp(c); }
  else if (tool === "fill") { flood(c); commitRows(); M.down = false; }
  else if (tool === "rect" || tool === "line" || tool === "region") M.drag = { from: c, to: c };
  else if (tool === "npc" && M.npc) { S.extra.npcs.push({ npc: M.npc, pos }); M.sel = { k: "npcs", i: S.extra.npcs.length - 1 }; done(); }
  else if (tool === "monster" && M.monster) {
    S.extra.monsters.push({ monster: M.monster, pos, count: M.count, radius: M.radius });
    M.sel = { k: "monsters", i: S.extra.monsters.length - 1 }; done();
  } else if (tool === "node" && M.node) { S.extra.nodes.push({ node: M.node, pos }); M.sel = { k: "nodes", i: S.extra.nodes.length - 1 }; done(); }
  else if (tool === "spawn" && S.extra.zone) { S.extra.zone.spawn = pos; done(); }
  else { M.undo.pop(); M.down = false; }
  paint();
}

function done() { M.down = false; changed(); redraw(); }

function onMove(e) {
  const c = cell(e);
  M.hover = c;
  if (M.down && M.tool === "paint" && M.grid2) { lineCells(M.last ?? c, c).forEach(stamp); }
  if (M.down && M.tool === "select" && M.moving) moveSel(c);
  if (M.drag) M.drag.to = c;
  M.last = c;
  schedulePaint();
  info(c);
}

function onUp() {
  if (M.drag && M.down) {
    const { from, to } = M.drag;
    if (M.tool === "region" && S.extra.zone) {
      const a = global({ x: Math.min(from.x, to.x), y: Math.min(from.y, to.y) });
      const b = global({ x: Math.max(from.x, to.x), y: Math.max(from.y, to.y) });
      (S.extra.zone.regions ||= []).push({ kind: M.region, min: a, max: b });
      M.sel = { k: "regions", i: S.extra.zone.regions.length - 1 };
      M.drag = null; M.down = false; changed(); redraw(); return;
    }
    M.grid2 = mapRows();
    const cells = M.tool === "rect" ? rectCells(from, to) : lineCells(from, to);
    cells.forEach(M.tool === "line" ? stamp : put);
    commitRows();
    M.drag = null;
  }
  if (M.down && M.tool === "paint" && M.grid2) commitRows();
  if (M.down && M.tool === "select" && M.moving) { M.moving = null; changed(); redraw(); }
  M.down = false;
}

function commitRows() {
  if (!M.grid2) return;
  S.value.rows = M.grid2.map((r) => r.join(""));
  M.grid2 = null;
  changed();
  paint();
  // A wall painted open (or closed) changes the passages on top.
  $("joinbar")?.replaceChildren(...[joinBanner()].filter(Boolean));
}

function put(c) {
  const g = M.grid2;
  if (g[c.y] && c.x >= 0 && c.x < g[c.y].length) g[c.y][c.x] = M.ch;
}

function stamp(c) {
  const half = (M.brush - 1) / 2;
  for (let y = c.y - half; y <= c.y + half; y++) for (let x = c.x - half; x <= c.x + half; x++) put({ x, y });
}

// Every cell from a to b: a fast stroke leaves no gaps.
function lineCells(a, b) {
  const n = Math.max(Math.abs(b.x - a.x), Math.abs(b.y - a.y), 1);
  const out = [];
  for (let i = 0; i <= n; i++) out.push({ x: Math.round(a.x + (b.x - a.x) * i / n), y: Math.round(a.y + (b.y - a.y) * i / n) });
  return out;
}

function rectCells(a, b) {
  const out = [];
  for (let y = Math.min(a.y, b.y); y <= Math.max(a.y, b.y); y++)
    for (let x = Math.min(a.x, b.x); x <= Math.max(a.x, b.x); x++) out.push({ x, y });
  return out;
}

// Bucket: every tile joined to this one with the same character.
function flood(c) {
  const g = M.grid2 = mapRows();
  const from = g[c.y]?.[c.x];
  if (from === undefined || from === M.ch) return;
  const stack = [c];
  while (stack.length) {
    const p = stack.pop();
    if (g[p.y]?.[p.x] !== from) continue;
    g[p.y][p.x] = M.ch;
    stack.push({ x: p.x + 1, y: p.y }, { x: p.x - 1, y: p.y }, { x: p.x, y: p.y + 1 }, { x: p.x, y: p.y - 1 });
  }
}

// ---------- selection ----------

function listOf(k) { return k === "regions" ? (S.extra.zone?.regions || []) : S.extra[k]; }

// What is under a tile: a spawn first, then the topmost region, then the
// player spawn.
function hit(c) {
  const p = global(c);
  const near = (q) => q.x === p.x && q.y === p.y;
  for (const k of ["npcs", "nodes", "monsters"]) {
    const i = S.extra[k].findIndex((s) => near(s.pos));
    if (i >= 0) return { k, i };
  }
  if (S.extra.zone && near(S.extra.zone.spawn)) return { k: "spawn", i: 0 };
  const regions = S.extra.zone?.regions || [];
  for (let i = regions.length - 1; i >= 0; i--) {
    const r = regions[i];
    if (p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y) return { k: "regions", i };
  }
  return null;
}

function startSelect(c) {
  M.sel = hit(c);
  if (M.sel) { remember(); M.moving = { from: c }; } else M.down = false;
  redraw();
}

function moveSel(c) {
  const dx = c.x - M.moving.from.x, dy = c.y - M.moving.from.y;
  if (!dx && !dy) return;
  M.moving.from = c;
  const { k, i } = M.sel;
  const mv = (p) => { p.x += dx; p.y += dy; };
  if (k === "spawn") mv(S.extra.zone.spawn);
  else if (k === "regions") { const r = listOf(k)[i]; mv(r.min); mv(r.max); }
  else mv(listOf(k)[i].pos);
}

function deleteSel() {
  if (!M.sel || M.sel.k === "spawn") return;
  remember();
  listOf(M.sel.k).splice(M.sel.i, 1);
  M.sel = null;
  changed(); redraw();
}

// Scrolls the canvas to the selected thing.
function focusSel() {
  const s = M.sel;
  if (!s) return;
  const it = s.k === "spawn" ? { pos: S.extra.zone.spawn } : listOf(s.k)[s.i];
  const p = it?.pos ?? it?.min;
  if (!p) return;
  const o = origin();
  setTimeout(() => {
    const wrap = $("mapwrap");
    if (!wrap) return;
    wrap.scrollLeft = (p.x - o.x + PAD) * M.zoom - wrap.clientWidth / 2;
    wrap.scrollTop = (p.y - o.y + PAD) * M.zoom - wrap.clientHeight / 2;
  });
}

// Right button: what is under the cursor goes away.
function eraseAt(c) {
  const h = hit(c);
  if (!h || h.k === "spawn") return;
  M.sel = h;
  deleteSel();
}

function resize(w, h, anchor) {
  if (!(w > 0 && h > 0)) return;
  const rows = mapRows();
  const { w: ow, h: oh } = mapSize();
  const fill = M.ch ?? ".";
  // Growing at the start (left and top) moves the origin, so whatever is
  // placed keeps its place in the world.
  const dx = anchor === "start" ? w - ow : 0, dy = anchor === "start" ? h - oh : 0;
  const out = [];
  for (let y = 0; y < h; y++) {
    const r = rows[y - dy] ?? [];
    out.push(Array.from({ length: w }, (_, x) => r[x - dx] ?? fill).join(""));
  }
  S.value.rows = out;
  if (dx || dy) S.extra.zone.origin = { x: S.extra.zone.origin.x - dx, y: S.extra.zone.origin.y - dy };
}

// ---------- drawing ----------

function schedulePaint() {
  if (M.frame) return;
  M.frame = requestAnimationFrame(() => { M.frame = 0; paint(); });
}

function info(c) {
  const box = $("mapinfo");
  if (!box) return;
  const rows = S.value.rows;
  const ch = Array.from(rows[c.y] || "")[c.x];
  if (ch === undefined) { box.textContent = t("editor.map.hover_hint"); return; }
  const def = (S.value.legend || {})[ch];
  const p = global(c);
  const parts = [`(${p.x}, ${p.y})`, def ? tileName(def.floor) : `'${ch}' ?`];
  if (def?.blocks_movement) parts.push(t("editor.map.tag_blocks"));
  const h = hit(c);
  if (h) {
    const it = h.k === "spawn" ? null : listOf(h.k)[h.i];
    parts.push(h.k === "spawn" ? t("editor.map.spawn")
      : h.k === "regions" ? vlabel(it.kind)
        : h.k === "npcs" ? refTitle("npc", it.npc)
          : h.k === "nodes" ? refTitle("gather_node", it.node) : refTitle("monster", it.monster));
  }
  if (M.seam && inSeam(p)) parts.push(t("editor.map.in_seam"));
  box.textContent = parts.join(" · ");
}

// Around the map: the tiles of the zones that touch it (faded, with their
// name), and a green mark where the edge can be crossed.
function paintNeighbors(g, w, h) {
  const z = M.zoom, o = origin();
  const near = neighbors();
  for (const n of near) {
    g.globalAlpha = .5;
    for (let gy = Math.max(n.y, o.y - PAD); gy < Math.min(n.y + n.h, o.y + h + PAD); gy++) {
      for (let gx = Math.max(n.x, o.x - PAD); gx < Math.min(n.x + n.w, o.x + w + PAD); gx++) {
        const d = n.legend[n.rows[gy - n.y]?.[gx - n.x]];
        g.fillStyle = d ? (M.colors[d.floor] || "#444") : "#f0f";
        g.fillRect((gx - o.x) * z, (gy - o.y) * z, z, z);
        if (d?.blocks_movement && z >= 6) { g.fillStyle = "rgba(0,0,0,.4)"; g.fillRect((gx - o.x) * z + z * .2, (gy - o.y) * z + z * .2, z * .6, z * .6); }
      }
    }
    g.globalAlpha = 1;
    // The neighbor's name, just outside the shared edge.
    const lx = Math.max(n.x, o.x - PAD) - o.x, ly = Math.max(n.y, o.y - PAD) - o.y;
    g.font = "bold 12px system-ui";
    const tw = g.measureText(n.name).width;
    g.fillStyle = "rgba(0,0,0,.75)"; g.fillRect(lx * z + 2, ly * z + 2, tw + 8, 16);
    g.fillStyle = "#e8ecf2"; g.fillText(n.name, lx * z + 6, ly * z + 14);
  }
  // Edge of this zone, and the passages over it.
  g.strokeStyle = "#3fb8ff"; g.lineWidth = 2;
  g.strokeRect(-1, -1, w * z + 2, h * z + 2);
  g.lineWidth = 1;
  for (const n of near) {
    for (const p of passages(n)) {
      g.fillStyle = "#5fd38d";
      const px = (p.x - o.x) * z, py = (p.y - o.y) * z;
      if (p.vertical) g.fillRect(px - 3, py, 6, z);
      else g.fillRect(px, py - 3, z, 6);
    }
  }
}

function paint() {
  const c = $("map");
  if (!c) return;
  const rows = M.grid2 ?? mapRows();
  const z = M.zoom, o = origin();
  const W = (rows[0]?.length ?? 0) * z, H = rows.length * z;
  if (c.width !== W + 2 * PAD * z) c.width = W + 2 * PAD * z;
  if (c.height !== H + 2 * PAD * z) c.height = H + 2 * PAD * z;
  const g = c.getContext("2d");
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.fillStyle = "#07090c"; g.fillRect(0, 0, c.width, c.height);
  g.translate(PAD * z, PAD * z);
  paintNeighbors(g, rows[0]?.length ?? 0, rows.length);
  const legend = S.value.legend || {};
  rows.forEach((r, y) => r.forEach((ch, x) => {
    const def = legend[ch];
    g.fillStyle = def ? (M.colors[def.floor] || "#444") : "#f0f";
    g.fillRect(x * z, y * z, z, z);
    if (def?.blocks_movement && z >= 6) {
      g.fillStyle = "rgba(0,0,0,.28)";
      g.fillRect(x * z + z * .2, y * z + z * .2, z * .6, z * .6);
    }
  }));
  if (M.grid && z >= 8) {
    g.strokeStyle = "rgba(0,0,0,.18)"; g.lineWidth = 1;
    g.beginPath();
    for (let x = 0; x <= rows[0]?.length; x++) { g.moveTo(x * z + .5, 0); g.lineTo(x * z + .5, H); }
    for (let y = 0; y <= rows.length; y++) { g.moveTo(0, y * z + .5); g.lineTo(W, y * z + .5); }
    g.stroke();
  }
  if (M.seam) {
    for (const b of seamBands()) {
      g.fillStyle = "rgba(255,225,77,.10)";
      g.fillRect(b.x * z, b.y * z, b.w * z, b.h * z);
    }
  }
  const at = (p) => ({ x: (p.x - o.x) * z, y: (p.y - o.y) * z });
  const rect = (min, max, color, dash, fill) => {
    const a = at(min);
    const w = (max.x - min.x + 1) * z, h = (max.y - min.y + 1) * z;
    if (fill) { g.fillStyle = fill; g.fillRect(a.x, a.y, w, h); }
    g.setLineDash(dash ? [4, 3] : []);
    g.strokeStyle = color; g.lineWidth = 2;
    g.strokeRect(a.x + 1, a.y + 1, w - 2, h - 2);
    g.setLineDash([]); g.lineWidth = 1;
  };
  const sel = (k, i) => M.sel && M.sel.k === k && M.sel.i === i;
  (S.extra.zone?.regions || []).forEach((r, i) => {
    rect(r.min, r.max, REGION_COLORS[r.kind] || "#fff", false, sel("regions", i) ? "rgba(255,255,255,.12)" : null);
    if (z >= 6) { const a = at(r.min); g.fillStyle = REGION_COLORS[r.kind]; g.font = "bold 11px system-ui"; g.fillText(vlabel(r.kind), a.x + 4, a.y + 13); }
  });
  if (M.drag) {
    const { from, to } = M.drag;
    const a = global({ x: Math.min(from.x, to.x), y: Math.min(from.y, to.y) });
    const b = global({ x: Math.max(from.x, to.x), y: Math.max(from.y, to.y) });
    if (M.tool === "region") rect(a, b, REGION_COLORS[M.region], true);
    else {
      const cells = M.tool === "rect" ? rectCells(from, to) : lineCells(from, to);
      const def = legend[M.ch];
      g.fillStyle = def ? M.colors[def.floor] || "#888" : "#888";
      g.globalAlpha = .7;
      const half = M.tool === "line" ? (M.brush - 1) / 2 : 0;
      for (const p of cells) g.fillRect((p.x - half) * z, (p.y - half) * z, z * (half * 2 + 1), z * (half * 2 + 1));
      g.globalAlpha = 1;
    }
  }
  const label = (a, text) => {
    if (z < 6) return;
    g.font = "11px system-ui";
    const w = g.measureText(text).width;
    g.fillStyle = "rgba(0,0,0,.6)"; g.fillRect(a.x + z + 1, a.y - 11, w + 6, 14);
    g.fillStyle = "#fff"; g.fillText(text, a.x + z + 4, a.y);
  };
  const dot = (p, color, text, on, shape) => {
    const a = at(p);
    const cx = a.x + z / 2, cy = a.y + z / 2, r = Math.max(3, z / 2.4);
    g.fillStyle = color;
    g.beginPath();
    if (shape === "diamond") { g.moveTo(cx, cy - r); g.lineTo(cx + r, cy); g.lineTo(cx, cy + r); g.lineTo(cx - r, cy); g.closePath(); }
    else g.arc(cx, cy, r, 0, 7);
    g.fill();
    if (on) { g.strokeStyle = "#fff"; g.lineWidth = 2; g.stroke(); g.lineWidth = 1; }
    if (M.seam && inSeam(p)) { g.strokeStyle = "#ff5d5d"; g.lineWidth = 2; g.strokeRect(a.x, a.y, z, z); g.lineWidth = 1; }
    label(a, text);
  };
  S.extra.monsters.forEach((m, i) => {
    const a = at(m.pos), rr = m.radius ?? 0;
    g.strokeStyle = "rgba(200,60,200,.7)";
    g.strokeRect(a.x - rr * z, a.y - rr * z, (2 * rr + 1) * z, (2 * rr + 1) * z);
    dot(m.pos, "#c03cc0", `${refTitle("monster", m.monster)} ×${m.count ?? 1}`, sel("monsters", i));
  });
  S.extra.nodes.forEach((n, i) => dot(n.pos, "#4fd1c5", refTitle("gather_node", n.node), sel("nodes", i), "diamond"));
  S.extra.npcs.forEach((n, i) => dot(n.pos, "#ff8a5c", refTitle("npc", n.npc), sel("npcs", i)));
  if (S.extra.zone) {
    const s = S.extra.zone.spawn, rr = S.extra.zone.spawn_radius ?? 0;
    const a = at(s);
    g.strokeStyle = "#ffe14d";
    g.strokeRect(a.x - rr * z, a.y - rr * z, (2 * rr + 1) * z, (2 * rr + 1) * z);
    dot(s, "#ffe14d", t("editor.map.spawn"), sel("spawn", 0));
  }
  const h = M.hover;
  if (h && rows[h.y]?.[h.x] !== undefined && !M.drag) {
    g.strokeStyle = "#fff";
    const b = M.tool === "paint" ? M.brush : 1, half = (b - 1) / 2;
    g.strokeRect((h.x - half) * z + .5, (h.y - half) * z + .5, z * b - 1, z * b - 1);
  }
}
