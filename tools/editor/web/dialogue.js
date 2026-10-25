// Dialogue editor of the content editor.
// A dialogue is steps (nodes): what the NPC says and the player's answers
// (options). Each answer may need a condition, run effects and go to
// another step or close. The page shows it the way a writer thinks of it:
// a map of the steps, one card per step with its answers written as
// sentences, and a "play" box that walks through it like the player.
"use strict";

const D = { of: null, open: {}, play: null, playPath: [] };

// ---------- sentences ----------

const dref = (kind, id) => (id ? refTitle(kind, id) : "?");

// A condition as a sentence, nested ones in parentheses.
function condText(c) {
  if (!c || typeof c !== "object") return "";
  const [k, v] = Object.entries(c)[0] || [];
  switch (k) {
    case "Item": return fmt("editor.dlg.c.item", { qty: v.qty, item: dref("item", v.item) });
    case "Coins": return fmt("editor.dlg.c.coins", { n: v });
    case "Quest": return fmt("editor.dlg.c.quest_" + String(v.state).toLowerCase(), { quest: dref("quest", v.quest) });
    case "QuestReady": return fmt("editor.dlg.c.quest_ready", { quest: dref("quest", v) });
    case "Affinity": return fmt("editor.dlg.c.affinity", { n: v });
    case "RecipeKnown": return fmt("editor.dlg.c.recipe", { recipe: dref("recipe", v) });
    case "TechniqueKnown": return fmt("editor.dlg.c.technique", { technique: dref("technique", v) });
    case "All": return (v || []).map(condText).map((s) => (v.length > 1 ? `(${s})` : s)).join(` ${t("editor.dlg.and")} `);
    case "Any": return (v || []).map(condText).map((s) => (v.length > 1 ? `(${s})` : s)).join(` ${t("editor.dlg.or")} `);
    case "Not": return fmt("editor.dlg.c.not", { c: condText(v) });
    default: return k || "";
  }
}

function effectText(e) {
  if (!e || typeof e !== "object") return String(e ?? "");
  const [k, v] = Object.entries(e)[0] || [];
  switch (k) {
    case "GiveItem": return fmt("editor.dlg.e.give_item", { qty: v.qty, item: dref("item", v.item) });
    case "TakeItem": return fmt("editor.dlg.e.take_item", { qty: v.qty, item: dref("item", v.item) });
    case "TopUpItem": return fmt("editor.dlg.e.top_up", { qty: v.qty, item: dref("item", v.item) });
    case "GiveCoins": return fmt("editor.dlg.e.give_coins", { n: v });
    case "TakeCoins": return fmt("editor.dlg.e.take_coins", { n: v });
    case "StartQuest": return fmt("editor.dlg.e.start_quest", { quest: dref("quest", v) });
    case "CompleteQuest": return fmt("editor.dlg.e.complete_quest", { quest: dref("quest", v) });
    case "Affinity": return fmt("editor.dlg.e.affinity", { n: v > 0 ? `+${v}` : v });
    case "LearnRecipe": return fmt("editor.dlg.e.recipe", { recipe: dref("recipe", v) });
    case "LearnTechnique": return fmt("editor.dlg.e.technique", { technique: dref("technique", v) });
    default: return k || "";
  }
}

// ---------- shape of the dialogue ----------

// Steps in reading order: from the start, breadth first; the ones nothing
// reaches go last.
function stepOrder(d) {
  const nodes = d.nodes || {};
  const seen = new Set();
  const out = [];
  const queue = nodes[d.start] ? [d.start] : [];
  while (queue.length) {
    const n = queue.shift();
    if (seen.has(n) || !nodes[n]) continue;
    seen.add(n); out.push(n);
    for (const o of nodes[n].options || []) if (o.next) queue.push(o.next);
  }
  const lost = Object.keys(nodes).filter((n) => !seen.has(n));
  return { order: out.concat(lost), lost: new Set(lost) };
}

// Steps that point at this one.
function cameFrom(d, name) {
  return Object.entries(d.nodes || {}).filter(([, n]) => (n.options || []).some((o) => o.next === name)).map(([k]) => k);
}

function dialogueProblems(d) {
  const out = [];
  const nodes = d.nodes || {};
  if (!nodes[d.start]) out.push(t("editor.dlg.p.no_start"));
  for (const [name, n] of Object.entries(nodes)) {
    if (!(n.options || []).length) out.push(fmt("editor.dlg.p.no_options", { step: name }));
    for (const o of n.options || []) if (o.next && !nodes[o.next]) out.push(fmt("editor.dlg.p.bad_next", { step: name, next: o.next }));
  }
  for (const n of stepOrder(d).lost) out.push(fmt("editor.dlg.p.lost", { step: n }));
  return out;
}

// ---------- the page ----------

function dialogueEditor() {
  const d = S.value;
  // Another dialogue: forget the open rules and where the play was.
  if (D.of !== S.sel) { D.of = S.sel; D.open = {}; D.play = null; D.playPath = []; }
  d.nodes ||= {};
  const root = S.schemas.dialogue;
  const { order, lost } = stepOrder(d);
  const out = [];

  // Start and problems.
  const problems = dialogueProblems(d);
  out.push(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.dlg.title")), el("div", { class: "grow" }),
      el("button", { class: "primary", onclick: () => { addStep(null); } }, "+ ", t("editor.dlg.add_step"))),
    el("p", { class: "hint" }, t("editor.dlg.intro")),
    el("div", { class: "f" }, el("label", {}, t("editor.dlg.start")),
      el("select", { onchange: (e) => { d.start = e.target.value; changed(); redraw(); } },
        Object.keys(d.nodes).map((n) => el("option", { value: n, selected: n === d.start }, n)))),
    problems.length ? el("ul", { class: "dlg-problems" }, problems.map((p) => el("li", {}, p))) : null,
    flowMap(d, order, lost)));

  for (const name of order) out.push(stepCard(root, d, name, lost.has(name)));
  out.push(playBox(d));
  return out;
}

// The steps as columns by distance from the start; each chip lists where
// its answers go. Clicking a chip scrolls to its card.
function flowMap(d, order, lost) {
  const depth = {};
  const nodes = d.nodes;
  if (nodes[d.start]) {
    depth[d.start] = 0;
    const q = [d.start];
    while (q.length) {
      const n = q.shift();
      for (const o of nodes[n].options || []) {
        if (o.next && nodes[o.next] && depth[o.next] === undefined) { depth[o.next] = depth[n] + 1; q.push(o.next); }
      }
    }
  }
  const cols = [];
  for (const n of order) {
    const c = lost.has(n) ? "lost" : depth[n];
    (cols[c === "lost" ? 999 : c] ||= []).push(n);
  }
  const box = el("div", { class: "flow" });
  for (const [i, col] of cols.entries()) {
    if (!col) continue;
    box.append(el("div", { class: "flow-col" },
      el("div", { class: "flow-h" }, i === 999 ? t("editor.dlg.unreached") : i === 0 ? t("editor.dlg.start") : fmt("editor.dlg.depth", { n: i })),
      col.map((n) => {
        const node = nodes[n];
        const goes = (node.options || []).map((o) => o.next || "✕");
        return el("a", { class: "flow-chip" + (n === d.start ? " start" : "") + (lost.has(n) ? " lost" : ""),
          onclick: () => document.getElementById("step-" + n)?.scrollIntoView({ behavior: "smooth", block: "start" }) },
        el("b", {}, n), el("span", {}, shortText(textOf(node.text) || node.text || "", 48)),
        el("span", { class: "flow-to" }, "→ " + [...new Set(goes)].join(", ")));
      })));
  }
  return box;
}

function shortText(s, n) { return s.length > n ? s.slice(0, n - 1) + "…" : s; }

function stepCard(root, d, name, isLost) {
  const node = d.nodes[name];
  node.options ||= [];
  const from = cameFrom(d, name);
  const head = el("div", { class: "card-head" },
    el("h3", {}, el("span", { class: "step-id" }, name)),
    name === d.start ? el("span", { class: "tag ok" }, t("editor.dlg.is_start")) : null,
    isLost ? el("span", { class: "tag warn" }, t("editor.dlg.unreached")) : null,
    el("div", { class: "grow" }),
    el("span", { class: "hint" }, from.length ? fmt("editor.dlg.from", { steps: from.join(", ") }) : ""),
    el("button", { class: "icon-btn", title: t("editor.dlg.rename"), onclick: () => renameStep(name) }, "✎"),
    el("button", { class: "icon-btn danger", title: t("editor.remove"), onclick: () => removeStep(name) }, "×"));
  const card = el("section", { class: "card step" + (name === d.start ? " start" : ""), id: "step-" + name }, head);
  card.append(el("div", { class: "f" }, el("label", {}, t("editor.dlg.npc_says")),
    textRefInput(node.text, (v) => { node.text = v; changed(); }, keyPrefix())));
  card.append(el("label", { class: "opts-h" }, t("editor.dlg.answers")));
  node.options.forEach((o, i) => card.append(optionRow(root, d, name, node, o, i)));
  card.append(el("button", { class: "ghost", onclick: () => {
    node.options.push({ text: "" }); changed(); redraw();
  } }, "+ ", t("editor.dlg.add_answer")));
  return card;
}

function optionRow(root, d, stepName, node, o, i) {
  const key = `${stepName}/${i}`;
  const open = !!D.open[key];
  const optS = R(root, R(root, root.properties.nodes.additionalProperties).properties.options.items);
  const next = el("select", { class: "next-pick", onchange: (e) => {
    const v = e.target.value;
    if (v === "+") { addStep(o); return; }
    o.next = v || undefined; changed(); redraw();
  } },
  el("option", { value: "", selected: !o.next }, "✕ " + t("editor.end_dialogue")),
  Object.keys(d.nodes).map((n) => el("option", { value: n, selected: o.next === n }, "→ " + n)),
  o.next && !d.nodes[o.next] ? el("option", { value: o.next, selected: true }, "→ " + o.next + " ?") : null,
  el("option", { value: "+" }, "+ " + t("editor.dlg.new_step")));
  const when = o.when ? el("div", { class: "cond" }, el("span", { class: "tag" }, t("editor.dlg.only_if")), condText(o.when)) : null;
  const effects = (o.effects || []).length
    ? el("div", { class: "effs" }, el("span", { class: "tag ok" }, t("editor.dlg.then")), (o.effects || []).map(effectText).join(" · "))
    : null;
  const move = (dir) => {
    const j = i + dir;
    if (j < 0 || j >= node.options.length) return;
    [node.options[i], node.options[j]] = [node.options[j], node.options[i]];
    changed(); redraw();
  };
  const row = el("div", { class: "opt" + (open ? " open" : "") },
    el("div", { class: "opt-main" },
      el("span", { class: "opt-n" }, `${i + 1}.`),
      el("div", { class: "opt-text" }, textRefInput(o.text, (v) => { o.text = v; changed(); }, keyPrefix() + "option.")),
      next,
      el("button", { class: "icon-btn", title: t("editor.dlg.rules"), onclick: () => { D.open[key] = !open; redraw(); } }, open ? "▾" : "⚙"),
      el("button", { class: "icon-btn", title: "↑", disabled: i === 0, onclick: () => move(-1) }, "↑"),
      el("button", { class: "icon-btn", title: "↓", disabled: i === node.options.length - 1, onclick: () => move(1) }, "↓"),
      el("button", { class: "icon-btn danger", title: t("editor.remove"), onclick: () => { node.options.splice(i, 1); changed(); redraw(); } }, "×")),
    when, effects);
  if (open) {
    row.append(el("div", { class: "opt-rules" },
      el("div", { class: "f" }, el("label", {}, t("editor.dlg.only_if"), el("span", { class: "hint" }, " — ", t("editor.dlg.only_if_hint"))),
        field(root, optS.properties.when, o.when ?? null, (v) => { o.when = v ?? undefined; changed(); }, "when")),
      el("div", { class: "f" }, el("label", {}, t("editor.dlg.then"), el("span", { class: "hint" }, " — ", t("editor.dlg.then_hint"))),
        field(root, optS.properties.effects, o.effects ?? [], (v) => { o.effects = v.length ? v : undefined; changed(); }, "effects"))));
  }
  return row;
}

// A new step, named from a free id; when an answer asked for it, the
// answer goes there.
async function addStep(fromOption) {
  const d = S.value;
  const name = await askStepName("");
  if (!name) { redraw(); return; }
  d.nodes[name] = { text: "", options: [{ text: "" }] };
  if (!d.start || !d.nodes[d.start]) d.start = name;
  if (fromOption) fromOption.next = name;
  changed(); redraw();
  setTimeout(() => document.getElementById("step-" + name)?.scrollIntoView({ behavior: "smooth", block: "start" }));
}

async function askStepName(current) {
  const d = S.value;
  const input = el("input", { class: "mono", value: current, autofocus: true, placeholder: "greeting" });
  const err = el("span", { class: "err-text" });
  const ok = () => {
    const v = slug(input.value);
    err.textContent = !v ? t("editor.bad_id") : v !== current && d.nodes[v] ? t("editor.id_taken") : "";
    return !err.textContent;
  };
  input.addEventListener("input", ok);
  const res = await dialog(current ? t("editor.dlg.rename") : t("editor.dlg.add_step"),
    [el("p", { class: "hint" }, t("editor.dlg.step_name_hint")), input, err],
    [{ label: t("editor.cancel"), value: false }, { label: t("editor.ok"), value: true, primary: true }], { validate: ok });
  return res ? slug(input.value) : null;
}

// Renames a step everywhere it is named (start and every answer).
async function renameStep(old) {
  const d = S.value;
  const name = await askStepName(old);
  if (!name || name === old) return;
  const nodes = {};
  for (const [k, v] of Object.entries(d.nodes)) nodes[k === old ? name : k] = v;
  d.nodes = nodes;
  if (d.start === old) d.start = name;
  for (const n of Object.values(nodes)) for (const o of n.options || []) if (o.next === old) o.next = name;
  changed(); redraw();
}

async function removeStep(name) {
  const d = S.value;
  const from = cameFrom(d, name);
  if (from.length) {
    const ok = await dialog(t("editor.dlg.remove_used"), [el("p", {}, fmt("editor.dlg.remove_used_text", { steps: from.join(", ") }))],
      [{ label: t("editor.cancel"), value: false }, { label: t("editor.remove"), value: true, primary: true }]);
    if (!ok) return;
    for (const n of Object.values(d.nodes)) for (const o of n.options || []) if (o.next === name) delete o.next;
  }
  delete d.nodes[name];
  if (d.start === name) d.start = Object.keys(d.nodes)[0] ?? "";
  changed(); redraw();
}

// ---------- play ----------

// Walks the dialogue as the player: the NPC line, the answers as buttons.
// Conditions are not checked (there is no player here); each answer says
// what it needs and does.
function playBox(d) {
  if (!D.play || !d.nodes[D.play]) { D.play = d.start; D.playPath = []; }
  const node = d.nodes[D.play];
  const box = el("section", { class: "card play" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.preview")), el("div", { class: "grow" }),
      el("button", { class: "ghost", onclick: () => { D.play = d.start; D.playPath = []; redraw(); } }, "↺ ", t("editor.dlg.restart"))),
    el("p", { class: "hint" }, t("editor.dlg.play_hint")));
  if (!node) return box;
  if (D.playPath.length) box.append(el("div", { class: "hint" }, [...D.playPath, D.play].join(" → ")));
  box.append(el("div", { class: "bubble" }, textOf(node.text) || el("i", { class: "hint" }, node.text || "…")));
  const list = el("div", { class: "play-opts" });
  (node.options || []).forEach((o, i) => {
    const notes = [o.when ? `${t("editor.dlg.only_if")}: ${condText(o.when)}` : "", (o.effects || []).map(effectText).join(" · ")].filter(Boolean);
    list.append(el("button", { class: "play-opt", onclick: () => {
      if (o.next && d.nodes[o.next]) { D.playPath.push(D.play); D.play = o.next; }
      else { D.play = d.start; D.playPath = []; toast(t("editor.end_dialogue")); }
      redraw();
    } }, el("span", {}, `${i + 1}. ${textOf(o.text) || o.text || "…"}`),
    notes.length ? el("small", { class: "hint" }, notes.join(" — ")) : null));
  });
  box.append(list);
  return box;
}
