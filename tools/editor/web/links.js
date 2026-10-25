// How content ties together, for the content editor
// Every file is read once (/api/all) and walked along its form schema:
// each field marked `x-ref` is a reference to another piece of content.
// From that index a page shows where its content is used in the game
// (where an NPC stands, who gives a quest, where an item comes from),
// the map opens on a placed thing, and a new id can be changed with every
// reference to it. Only lookups: no game rule lives here.
"use strict";

// ---------- the index ----------

// Calls visit(ref, value, keys, set) on every reference inside `v`.
// `keys` is the path from the top of the file; `set` replaces the value.
function walkRefs(root, s, v, keys, visit, set) {
  if (v === null || v === undefined || !s) return;
  const r = R(root, s);
  switch (shape(root, s)) {
    case "option": walkRefs(root, inner(root, s), v, keys, visit, set); return;
    case "struct":
      for (const [k, ps] of Object.entries(r.properties)) {
        if (v[k] !== undefined) walkRefs(root, ps, v[k], [...keys, k], visit, (nv) => { v[k] = nv; });
      }
      return;
    case "list": if (Array.isArray(v)) v.forEach((x, i) => walkRefs(root, r.items, x, [...keys, i], visit, (nv) => { v[i] = nv; })); return;
    case "map": for (const k of Object.keys(v)) walkRefs(root, r.additionalProperties, v[k], [...keys, k], visit, (nv) => { v[k] = nv; }); return;
    case "tuple": r.prefixItems.forEach((ps, i) => walkRefs(root, ps, v[i], [...keys, i], visit, (nv) => { v[i] = nv; })); return;
    case "enum": {
      if (typeof v !== "object") return;
      const name = Object.keys(v)[0];
      const vr = variants(root, s).find((x) => x.name === name);
      if (vr?.payload) walkRefs(root, vr.payload, v[name], [...keys, name], visit, (nv) => { v[name] = nv; });
      return;
    }
    default:
      if (r["x-ref"] && typeof v === "string" && v) visit(r["x-ref"], v, keys, set);
  }
}

// `kind:id` -> [{ f, keys }]: every place that names it. Built once per
// load of the content (reloadContent clears it).
async function linkIndex() {
  if (S.links) return S.links;
  const all = await api("/api/all");
  const idx = new Map();
  for (const f of all) {
    const root = S.schemas[f.kind];
    if (!root) continue;
    walkRefs(root, root, f.json, [], (ref, v, keys) => {
      const k = ref + ":" + v;
      if (!idx.has(k)) idx.set(k, []);
      idx.get(k).push({ f, keys });
    }, () => {});
  }
  S.links = { all, idx };
  return S.links;
}

function refsTo(kind, id) { return S.links?.idx.get(kind + ":" + id) || []; }
function fileOf(kind, id) { return S.links?.all.find((f) => f.kind === kind && f.id === id); }
function at(obj, keys) { return keys.reduce((o, k) => (o == null ? o : o[k]), obj); }
function pathOf(kind, id) { return kindInfo(kind)?.path.replace("{id}", id); }

// ---------- the "in the game" card ----------

// A clickable name of another piece of content.
function linkTo(kind, id, extra) {
  return el("button", { class: "lk", onclick: () => open(pathOf(kind, id)) },
    thumb(titleOf(kind, id), picOf(fileBrief(kind, id)), 20, kind), el("span", {}, titleOf(kind, id)), extra ? el("span", { class: "hint" }, extra) : null);
}
function fileBrief(kind, id) { return S.content.files.find((f) => f.kind === kind && f.id === id)?.brief; }
function lkRow(label, nodes, warn) {
  return el("div", { class: "lk-row" + (warn ? " warn" : "") }, el("div", { class: "lk-label" }, label),
    el("div", { class: "lk-list" }, nodes.length ? nodes : el("span", { class: warn ? "warn-text" : "hint" }, warn || t("editor.links.none"))));
}
const uniq = (xs, key) => { const seen = new Set(); return xs.filter((x) => { const k = key(x); if (seen.has(k)) return false; seen.add(k); return true; }); };

// Kinds that get the card, and what it shows.
const CONTEXT = { item: itemContext, npc: npcContext, quest: questContext, monster: placedContext, gather_node: placedContext, dialogue: dialogueContext, shop: shopContext };

async function contextCard(page, kind, id, sel) {
  if (!CONTEXT[kind]) return;
  await linkIndex();
  if (S.sel !== sel || !page.isConnected) return;
  const rows = CONTEXT[kind](kind, id);
  if (!rows.length) return;
  const card = el("section", { class: "card ctx" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.links.title")), el("span", { class: "card-hint" }, t("editor.links.hint"))), ...rows);
  // Right under the name and description.
  const basics = page.querySelector(".card");
  // The first card may sit inside a wrapper: insert beside it, wherever it is.
  if (basics) basics.after(card); else page.append(card);
}

// Where an item comes from (the way a player gets it) and what asks for it.
function itemContext(kind, id) {
  const from = [], used = [];
  for (const { f, keys } of refsTo("item", id)) {
    if (f.kind === "monster") from.push(linkTo("monster", f.id, t("editor.links.drop")));
    else if (f.kind === "shop") from.push(linkTo("shop", f.id, t("editor.links.sold")));
    else if (f.kind === "recipe") (keys[0] === "result" ? from : used).push(linkTo("recipe", f.id, keys[0] === "result" ? t("editor.links.crafted") : t("editor.links.material")));
    else if (f.kind === "loot_table") {
      for (const u of refsTo("loot_table", f.id)) from.push(linkTo(u.f.kind, u.f.id, u.f.kind === "gather_node" ? t("editor.links.gathered") : t("editor.links.drop")));
    } else if (f.kind === "quest") (keys.includes("rewards") ? from : used).push(linkTo("quest", f.id, keys.includes("rewards") ? t("editor.links.reward") : t("editor.links.asked")));
    else if (f.kind === "dialogue") {
      const give = keys.includes("GiveItem") || keys.includes("TopUpItem");
      (give ? from : used).push(linkTo("dialogue", f.id, give ? t("editor.links.given") : t("editor.links.taken")));
    } else if (kindInfo(f.kind)?.per_id) used.push(linkTo(f.kind, f.id));
  }
  return [lkRow(t("editor.links.from"), uniq(from, (n) => n.textContent), t("editor.links.no_source")), lkRow(t("editor.links.used"), uniq(used, (n) => n.textContent))];
}

// Where a placed thing stands: zone and tile, opening the map on it.
function places(kind, id) {
  const file = { npc: "zone_npcs", monster: "zone_monsters", gather_node: "zone_nodes" }[kind];
  const list = { zone_npcs: "npcs", zone_monsters: "monsters", zone_nodes: "nodes" }[file];
  return refsTo(kind, id).filter((h) => h.f.kind === file).map(({ f, keys }) => {
    const it = f.json[keys[0]];
    const extra = kind === "monster" ? ` ×${it.count ?? 1}` : "";
    return el("button", { class: "lk", onclick: () => openOnMap(f.id, list, keys[0]) },
      icon("map", 16), el("span", {}, titleOf("map", f.id)), el("span", { class: "hint" }, `(${it.pos.x}, ${it.pos.y})${extra}`));
  });
}

function placeRow(kind, id) {
  const here = places(kind, id);
  const add = S.isNew ? null : el("button", { class: "sm", onclick: () => placeOnMap(kind, id) }, icon("plus", 14), t("editor.links.place"));
  return el("div", { class: "lk-row" + (here.length ? "" : " warn") }, el("div", { class: "lk-label" }, t("editor.links.where")),
    el("div", { class: "lk-list" }, here.length ? here : el("span", { class: "warn-text" }, t("editor.links.nowhere")), add));
}

function placedContext(kind, id) {
  const rows = [placeRow(kind, id)];
  if (kind === "monster") {
    const qs = refsTo("monster", id).filter((h) => h.f.kind === "quest").map((h) => linkTo("quest", h.f.id));
    if (qs.length) rows.push(lkRow(t("editor.links.quests"), uniq(qs, (n) => n.textContent)));
  }
  return rows;
}

// The quests a dialogue starts, ends, or needs done first.
function questMoves() {
  const out = [];
  for (const f of S.links.all.filter((x) => x.kind === "dialogue")) {
    walkRefs(S.schemas.dialogue, S.schemas.dialogue, f.json, [], (ref, v, keys) => {
      if (ref !== "quest") return;
      const move = keys.includes("StartQuest") ? "start" : keys.includes("CompleteQuest") ? "end" : null;
      if (!move) return;
      // The answer that does it, and what it asks for (`when`).
      const opt = at(f.json, keys.slice(0, keys.indexOf("effects")));
      const needs = [];
      if (opt?.when) {
        walkRefs(S.schemas.dialogue, { $ref: "#/$defs/ConditionFile" }, opt.when, [], (r2, q, k2) => {
          if (r2 !== "quest" || !k2.includes("Quest") || k2.includes("Not")) return;
          if (at(opt.when, [...k2.slice(0, -1), "state"]) === "Completed") needs.push(q);
        }, () => {});
      }
      out.push({ dialogue: f.id, quest: v, move, needs });
    }, () => {});
  }
  return out;
}
function npcsOf(dialogue) { return refsTo("dialogue", dialogue).filter((h) => h.f.kind === "npc").map((h) => h.f.id); }
function whoLinks(moves) {
  return uniq(moves.flatMap((m) => { const n = npcsOf(m.dialogue); return n.length ? n.map((x) => linkTo("npc", x)) : [linkTo("dialogue", m.dialogue)]; }), (n) => n.textContent);
}

function questContext(kind, id) {
  const moves = questMoves();
  const mine = moves.filter((m) => m.quest === id);
  const starts = mine.filter((m) => m.move === "start"), ends = mine.filter((m) => m.move === "end");
  const before = uniq(starts.flatMap((m) => m.needs), (x) => x).map((q) => linkTo("quest", q));
  const after = uniq(moves.filter((m) => m.move === "start" && m.needs.includes(id)).map((m) => m.quest), (x) => x).map((q) => linkTo("quest", q));
  return [
    lkRow(t("editor.links.given_by"), whoLinks(starts), t("editor.links.no_giver")),
    lkRow(t("editor.links.ended_by"), whoLinks(ends), t("editor.links.no_end")),
    lkRow(t("editor.links.before"), before), lkRow(t("editor.links.after"), after),
  ];
}

function npcContext(kind, id) {
  const rows = [placeRow("npc", id)];
  const dlg = S.value?.dialogue;
  if (dlg) {
    const moves = questMoves().filter((m) => m.dialogue === dlg);
    const give = uniq(moves.filter((m) => m.move === "start").map((m) => m.quest), (x) => x).map((q) => linkTo("quest", q));
    const end = uniq(moves.filter((m) => m.move === "end").map((m) => m.quest), (x) => x).map((q) => linkTo("quest", q));
    rows.push(lkRow(t("editor.links.gives"), give), lkRow(t("editor.links.ends"), end));
  }
  const shop = S.value?.shop;
  if (shop) rows.push(lkRow(t("editor.links.sells"), (fileOf("shop", shop)?.json.stock || []).map((s) => linkTo("item", s.item))));
  return rows;
}

function dialogueContext(kind, id) {
  return [lkRow(t("editor.links.spoken_by"), npcsOf(id).map((n) => linkTo("npc", n)), t("editor.links.nobody"))];
}

function shopContext(kind, id) {
  return [lkRow(t("editor.links.kept_by"), refsTo("shop", id).filter((h) => h.f.kind === "npc").map((h) => linkTo("npc", h.f.id)), t("editor.links.nobody"))];
}

// ---------- the map ----------

// Opens a zone's map with one placed thing selected and in sight.
async function openOnMap(zone, list, i) {
  await open(`zones/${zone}/map.ron`);
  if (S.kind !== "map" || !S.extra) return;
  M.view = "zone"; M.tool = "select"; M.sel = { k: list, i: +i };
  redraw();
  focusSel();
}

// Asks which zone, then opens its map with the tool and the content chosen.
async function placeOnMap(kind, id) {
  const zones = S.content.ids.map || [];
  const last = storeGet("editor-ui.place_zone");
  const sel = el("select", { class: "big" }, zones.map((z) => el("option", { value: z, selected: z === last }, titleOf("map", z))));
  const zone = await dialog(fmt("editor.links.place_title", { name: titleOf(kind, id) }), [el("div", { class: "fld" }, el("label", {}, t("editor.links.zone")), sel)],
    [{ label: t("editor.cancel"), value: null }, { label: t("editor.links.open_map"), value: "go", primary: true }]);
  if (zone !== "go") return;
  storeSet("editor-ui.place_zone", sel.value);
  await open(`zones/${sel.value}/map.ron`);
  if (S.kind !== "map" || !S.extra) return;
  const tool = { npc: "npc", monster: "monster", gather_node: "node" }[kind];
  M.view = "zone"; M.tool = tool; M[tool] = id; M.sel = null;
  redraw();
  toast(t("editor.links.place_hint"));
}

// ---------- changing a new id ----------

// Only for a file git does not have yet: nobody has the old id. The file
// moves, every reference to it follows, and its name and description keep
// their translations under the new key.
async function renameId() {
  const kind = S.kind, old = idOfPath(kind, S.sel), info = kindInfo(kind);
  if (dirty()) { await say(t("editor.rename"), t("editor.rename_save_first")); return; }
  const taken = new Set(S.content.ids[kind] || []);
  const input = el("input", { class: "big mono", value: old, autofocus: true });
  const err = el("div", { class: "err-text" });
  const valid = () => {
    const v = slug(input.value);
    err.textContent = !v ? t("editor.rename_empty") : v !== old && taken.has(v) ? t("editor.id_taken") : "";
    return !!v && v !== old && !taken.has(v);
  };
  input.addEventListener("input", valid);
  const ok = await dialog(t("editor.rename"), [el("p", { class: "hint" }, t("editor.rename_hint")), input, err],
    [{ label: t("editor.cancel"), value: false }, { label: t("editor.rename"), value: true, primary: true }], { validate: valid });
  if (!ok) return;
  const nid = slug(input.value);
  await linkIndex();
  const body = { files: [{ path: pathOf(kind, nid), json: clean(S.value) }], texts: [], remove: [S.sel], remove_texts: [] };
  // Every other file that names the old id.
  for (const f of uniq(refsTo(kind, old).map((h) => h.f), (f) => f.path)) {
    if (f.path === S.sel) continue;
    const json = structuredClone(f.json);
    walkRefs(S.schemas[f.kind], S.schemas[f.kind], json, [], (ref, v, keys, set) => { if (ref === kind && v === old) set(nid); }, () => {});
    body.files.push({ path: f.path, json });
  }
  // The name and description keys come from the id.
  for (const tpl of info.texts) {
    const ok2 = tpl.replace("{id}", old), nk = tpl.replace("{id}", nid);
    const row = rowOf(ok2);
    if (!row) continue;
    for (const [locale, text] of Object.entries(row.texts)) {
      if (text) body.texts.push({ locale, key: nk, text, ...(locale === "en" ? { note: row.note || undefined, max_chars: row.max_chars || undefined } : {}) });
    }
    body.remove_texts.push(ok2);
  }
  const r = await api("/api/save", body);
  if (!r.ok) { S.saveReport = r.report; renderProblems(); toast(t("editor.fix_first"), "err"); return; }
  await Promise.all([loadTexts(), reloadContent()]);
  S.sel = null;
  await open(pathOf(kind, nid));
  toast(t("editor.renamed"));
}

function isNewFile(path) { return !!S.content.files.find((f) => f.path === path)?.new; }
