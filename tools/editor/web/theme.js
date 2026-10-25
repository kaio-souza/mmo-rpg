// Theme screen: colors, fonts, sizes and timings of the game's interface
// (assets/themes/<id>.ron). The server checks every save with the client's
// own theme loader; the running client reloads the file.
"use strict";

const TH = { info: null, id: "", theme: null, saved: "", problem: null };

// The colors in groups, as a designer looks for them.
const THEME_GROUPS = [
  ["editor.theme.group.interface", ["background", "panel", "border", "strong_border", "text", "dim_text", "highlight", "secondary", "system"]],
  ["editor.theme.group.bars", ["life", "mana", "stamina", "xp"]],
  ["editor.theme.group.states", ["error", "cooldown"]],
  ["editor.theme.group.rarity", ["rarity_rare", "rarity_epic", "rarity_unique", "rarity_legendary", "rarity_mythic"]],
];
const THEME_NUMBERS = [
  ["editor.theme.group.panel", "panel"],
  ["editor.theme.group.sizes", "sizes"],
  ["editor.theme.group.windows", "windows"],
  ["editor.theme.group.system", "system_window"],
];

async function loadTheme(id) {
  TH.info = await api("/api/themes");
  const want = id || storeGet("editor-ui.theme");
  TH.id = TH.info.themes.includes(want) ? want : (TH.info.themes.includes(TH.info.game_theme) ? TH.info.game_theme : TH.info.themes[0] || "");
  if (!TH.id) { TH.theme = null; return; }
  storeSet("editor-ui.theme", TH.id);
  const r = await api(`/api/theme/${TH.id}`);
  TH.theme = r.theme;
  TH.problem = r.problem;
  TH.saved = JSON.stringify(TH.theme);
}

function themeDirty() { return !!TH.theme && JSON.stringify(TH.theme) !== TH.saved; }

async function pickTheme(id) {
  if (id === TH.id) return;
  if (themeDirty() && !(await askLeave())) return;
  await loadTheme(id);
  draw();
}

async function newTheme() {
  const input = el("input", { placeholder: "dark_forest", maxlength: 40 });
  const ok = await dialog(t("editor.theme.new"), [
    el("p", { class: "hint" }, t("editor.theme.new_hint")),
    el("div", { class: "f" }, el("label", {}, t("editor.theme.name")), input),
  ], [
    { label: t("editor.cancel"), value: false },
    { label: t("editor.create"), value: true, primary: true },
  ], { validate: () => !!slug(input.value) });
  if (!ok) return;
  if (themeDirty() && !(await askLeave())) return;
  const id = slug(input.value);
  try {
    await api(`/api/theme/${id}/copy`, { from: TH.id });
  } catch (e) {
    await say(t("editor.theme.new"), e.message);
    return;
  }
  await loadTheme(id);
  toast(t("editor.theme.created"));
  draw();
}

async function useThemeInGame() {
  if (themeDirty()) await saveTheme();
  if (themeDirty()) return;
  try {
    TH.info = await api(`/api/theme/${TH.id}/use`, {});
  } catch (e) {
    await say(t("editor.theme.use"), e.message);
    return;
  }
  toast(fmt("editor.theme.used", { id: TH.id }));
  draw();
}

async function saveTheme() {
  let r;
  try {
    r = await api(`/api/theme/${TH.id}`, TH.theme);
  } catch (e) {
    await say(t("editor.theme"), e.message);
    return;
  }
  await loadTheme(TH.id);
  if (r.missing_fonts.length) toast(fmt("editor.theme.missing_fonts", { list: r.missing_fonts.join(", ") }), "err");
  else toast(t("editor.theme.saved"));
  draw();
}

function drawThemeSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.theme")),
    el("button", { class: "primary small", onclick: newTheme, disabled: !TH.id }, icon("plus", 15), t("editor.theme.new"))));
  const list = el("div", { class: "list" });
  for (const id of TH.info?.themes || []) {
    list.append(el("div", { class: "entry" + (id === TH.id ? " on" : ""), onclick: () => pickTheme(id) },
      el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(id)}` }, icon("palette", 18)),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, id),
        el("div", { class: "hint" }, id === TH.info.game_theme ? t("editor.theme.in_game") : `themes/${id}.ron`))));
  }
  side.append(list);
}

// "#rrggbb" or "#rrggbbaa" -> [rgb hex, alpha 0..255]; null when malformed.
function themeSplit(v) {
  const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(v || "");
  return m ? ["#" + m[1].toLowerCase(), m[2] ? parseInt(m[2], 16) : 255] : null;
}

function themeJoin(rgb, a) {
  return a >= 255 ? rgb : rgb + Math.round(a).toString(16).padStart(2, "0");
}

// The color a name shows: its own, or the one it follows (a name or a literal).
function themeColor(name, seen = 0) {
  const v = TH.theme.colors[name];
  if (v) return v;
  const spec = TH.info.colors.find((c) => c.name === name);
  const fb = spec?.fallback;
  if (!fb || seen > 4) return "#808080";
  return fb.startsWith("#") ? fb : themeColor(fb, seen + 1);
}

// CSS for a theme color (CSS reads #rrggbbaa too).
function themeCss(name) { return themeColor(name); }

function themeColorRow(spec) {
  const own = TH.theme.colors[spec.name];
  const shown = themeSplit(themeColor(spec.name)) || ["#808080", 255];
  const bad = own !== undefined && own !== "" && !themeSplit(own);
  const set = (v) => {
    if (v === "" && spec.fallback) delete TH.theme.colors[spec.name];
    else TH.theme.colors[spec.name] = v;
    drawThemeLive();
  };
  const pick = el("input", { type: "color", value: shown[0],
    oninput: (e) => { const a = themeSplit(themeColor(spec.name))?.[1] ?? 255; text.value = themeJoin(e.target.value, a); set(text.value); } });
  const alpha = el("input", { type: "range", min: 0, max: 255, value: shown[1], title: t("editor.theme.opacity"),
    oninput: (e) => { const rgb = themeSplit(themeColor(spec.name))?.[0] ?? "#808080"; text.value = themeJoin(rgb, +e.target.value); set(text.value); } });
  const text = el("input", { class: "mono" + (bad ? " bad" : ""), value: own ?? "", placeholder: spec.fallback ? themeColor(spec.name) : "#rrggbb",
    oninput: (e) => {
      const v = e.target.value.trim();
      const ok = themeSplit(v);
      e.target.classList.toggle("bad", !!v && !ok || (!v && !spec.fallback));
      if (ok) { pick.value = ok[0]; alpha.value = ok[1]; }
      set(v);
    } });
  const follows = spec.fallback && !own
    ? el("span", { class: "hint" }, fmt("editor.theme.follows", { name: spec.fallback }))
    : spec.fallback
      ? el("button", { class: "ghost small", onclick: () => { set(""); drawTheme(); } }, t("editor.theme.use_default"))
      : null;
  return el("div", { class: "theme-color" },
    el("label", { class: "mono" }, spec.name),
    el("div", { class: "theme-color-in" }, pick, alpha, text), follows);
}

function themeNumberRow(group, key, def) {
  const v = TH.theme[group][key];
  const label = el("label", { class: "mono" }, key);
  if (typeof def === "boolean") {
    return el("div", { class: "f" }, label,
      el("input", { type: "checkbox", checked: !!v, onchange: (e) => { TH.theme[group][key] = e.target.checked; drawThemeLive(); } }));
  }
  if (typeof def === "string") {
    return el("div", { class: "f" }, label,
      el("input", { class: "mono", value: v ?? "", oninput: (e) => { TH.theme[group][key] = e.target.value; drawThemeLive(); } }));
  }
  const step = Number.isInteger(def) && !["diagonal_slash", "border_thickness", "border_glow", "rise"].includes(key) ? 1 : 0.1;
  return el("div", { class: "f" }, label,
    el("input", { type: "number", step, min: 0, value: v,
      oninput: (e) => { const n = parseFloat(e.target.value); if (!Number.isNaN(n)) { TH.theme[group][key] = n; drawThemeLive(); } } }),
    el("span", { class: "hint" }, fmt("editor.theme.default_value", { v: def })));
}

function themeFontSelect(set, key, value) {
  const opts = [...new Set([...(TH.info.fonts || []), value].filter(Boolean))];
  return el("div", { class: "f" }, el("label", {}, t(`editor.theme.font.${key}`)),
    el("select", { onchange: (e) => { set[key] = e.target.value; drawThemeHead(); } },
      opts.map((f) => el("option", { value: f, selected: f === value }, f))));
}

// A small piece of the game's HUD in the theme's colors.
function themePreview() {
  const p = TH.theme.panel;
  const s = TH.theme.sizes;
  const cut = Math.max(0, +p.diagonal_slash || 0);
  const glow = Math.max(0, +p.border_glow || 0);
  const bar = (name, k) => el("div", { class: "tp-bar", style: `height:${s.bar_height}px;background:${themeCss("panel")};border:1px solid ${themeCss("border")}` },
    el("div", { style: `width:${k}%;background:${themeCss(name)}` }));
  const rarity = TH.info.colors.filter((c) => c.name.startsWith("rarity_"))
    .map((c) => el("span", { style: `color:${themeCss(c.name)}` }, c.name.slice(7)));
  const slot = (cd) => el("div", { class: "tp-slot", style: `width:${s.hotbar_slot}px;height:${s.hotbar_slot}px;border:${p.border_thickness}px solid ${themeCss("border")};background:${themeCss("panel")};color:${themeCss("text")}` },
    "Q", cd ? el("div", { class: "tp-cd", style: `height:${cd}%;background:${themeCss("cooldown")}` }) : null);
  const panel = el("div", { class: "tp-panel", style: [
    `background:${themeCss("panel")}`,
    `border:${p.border_thickness}px solid ${themeCss("strong_border")}`,
    `box-shadow:0 0 ${12 * glow}px ${themeCss("strong_border")}`,
    `clip-path:polygon(${cut}px 0,100% 0,100% calc(100% - ${cut}px),calc(100% - ${cut}px) 100%,0 100%,0 ${cut}px)`,
    `padding:${s.padding}px`,
  ].join(";") },
    el("div", { style: `color:${themeCss("highlight")};font-size:${s.title}px;font-weight:700` }, t("editor.theme.sample.title")),
    el("div", { style: `color:${themeCss("text")};font-size:${s.text}px` }, t("editor.theme.sample.text")),
    el("div", { style: `color:${themeCss("dim_text")};font-size:${s.small}px` }, t("editor.theme.sample.dim")),
    el("div", { class: "tp-rarity", style: `font-size:${s.small}px` }, rarity),
    bar("life", 72), bar("mana", 45), bar("stamina", 88), bar("xp", 30),
    el("div", { class: "tp-row" }, slot(0), slot(60), el("div", { style: `color:${themeCss("secondary")};font-size:${s.small}px` }, "secondary")));
  return el("div", { class: "tp-stage", style: `background:${themeCss("background")}` }, panel,
    el("div", { class: "tp-msgs", style: `font-size:${s.small}px` },
      el("div", { style: `color:${themeCss("system")}` }, t("editor.theme.sample.system")),
      el("div", { style: `color:${themeCss("error")}` }, t("editor.theme.sample.error"))));
}

function drawTheme() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page" });
  if (!TH.theme) {
    page.append(el("p", { class: "hint" }, t("editor.theme.none")));
    p.append(page);
    return;
  }
  const dirtyNow = themeDirty();
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, `assets/themes/${TH.id}.ron`), el("h1", {}, `${t("editor.theme")} · ${TH.id}`)),
    el("div", { class: "grow" }),
    TH.id === TH.info.game_theme
      ? el("span", { class: "chip ok" }, icon("check", 14), t("editor.theme.in_game"))
      : el("button", { class: "keep", onclick: useThemeInGame }, icon("crown", 16), t("editor.theme.use")),
    el("button", { class: "ghost", disabled: !dirtyNow, onclick: () => { TH.theme = JSON.parse(TH.saved); draw(); } }, t("editor.looks.discard")),
    el("button", { class: "primary", disabled: !dirtyNow, onclick: saveTheme }, icon("check", 16), t("editor.save"))));
  page.append(el("p", { class: "hint" }, t("editor.theme.intro"), " ", fmt("editor.theme.game_uses", { id: TH.info.game_theme })));
  if (TH.problem) page.append(el("div", { class: "alert err" }, el("div", { class: "alert-head" }, icon("alert", 16), fmt("editor.theme.problem", { e: TH.problem }))));

  page.append(el("section", { class: "card theme-sticky" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.theme.preview"))),
    el("div", { id: "theme-preview" }, themePreview())));

  const colors = el("section", { class: "card", id: "theme-colors" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.theme.colors"))),
    el("p", { class: "hint" }, t("editor.theme.colors_hint")));
  for (const [label, names] of THEME_GROUPS) {
    colors.append(el("h4", { class: "theme-group" }, t(label)));
    for (const n of names) {
      const spec = TH.info.colors.find((c) => c.name === n);
      if (spec) colors.append(themeColorRow(spec));
    }
  }
  page.append(colors);

  const fonts = el("section", { class: "card", id: "theme-fonts" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.theme.fonts"))),
    el("p", { class: "hint" }, t("editor.theme.fonts_hint")),
    el("div", { class: "fields" }, ["title", "text", "numbers"].map((k) => themeFontSelect(TH.theme.fonts, k, TH.theme.fonts[k]))));
  for (const [l, set] of Object.entries(TH.theme.fonts_by_locale || {})) {
    fonts.append(el("h4", { class: "theme-group" }, `${langName(l, S.uiLocale)} · ${l}`),
      el("div", { class: "fields" }, ["title", "text", "numbers"].map((k) => themeFontSelect(set, k, set[k]))));
  }
  page.append(fonts);

  const nums = el("section", { class: "card", id: "theme-layout" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.theme.layout"))),
    el("p", { class: "hint" }, t("editor.theme.layout_hint")));
  for (const [label, group] of THEME_NUMBERS) {
    const defs = TH.info.defaults[group] || {};
    nums.append(el("h4", { class: "theme-group" }, t(label)),
      el("div", { class: "fields" }, Object.keys(defs).map((k) => themeNumberRow(group, k, defs[k]))));
  }
  page.append(nums);
  p.append(page);
}

// Typing keeps the focus: only the preview and the buttons follow.
function drawThemeLive() {
  const box = $("theme-preview");
  if (box) box.replaceChildren(themePreview());
  drawThemeHead();
}

function drawThemeHead() {
  const dirtyNow = themeDirty();
  for (const b of $("panel").querySelectorAll(".page-head button:not(.keep)")) b.disabled = !dirtyNow;
}
