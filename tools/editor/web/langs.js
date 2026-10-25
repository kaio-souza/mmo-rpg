// Languages: the list of content/text/locales.ron, which ones are
// required, and how much of each is translated (content texts and the
// client's interface texts). The list is saved like any other file.

const G = { info: [], list: [], saved: "" };

// Codes i18n has plural and number rules for (shared/i18n/src/locale.rs):
// offered when adding a language.
const KNOWN_CODES = ["en", "pt-BR", "pt-PT", "es", "fr", "it", "de", "nl", "sv", "da", "nb", "fi", "el", "hu", "tr", "ro",
  "ru", "uk", "be", "pl", "cs", "sk", "zh", "ja", "ko", "id", "ms", "vi", "th"];

const CODE_RE = /^[A-Za-z][A-Za-z0-9-]{1,14}$/;

async function loadLangs() {
  G.info = await api("/api/locales");
  G.list = G.info.map((l) => ({ code: l.code, required: l.required }));
  G.saved = JSON.stringify(G.list);
}

function langsDirty() { return JSON.stringify(G.list) !== G.saved; }

function infoOf(code) { return G.info.find((l) => l.code === code); }

function pct(pair) { return pair[1] ? Math.round((100 * pair[0]) / pair[1]) : 100; }

function drawLangsSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.languages"))));
  const list = el("div", { class: "list" });
  for (const l of G.list) {
    const i = infoOf(l.code);
    list.append(el("div", { class: "entry" },
      el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(l.code)}` }, l.code.slice(0, 2).toUpperCase()),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, `${langName(l.code, l.code)} · ${l.code}`),
        el("div", { class: "bar" }, el("span", { style: `width:${i ? pct(i.content) : 0}%` }))),
      el("span", { class: "chip" + (l.required || l.code === "en" ? "" : " warn") }, t(l.required ? "editor.langs.required" : "editor.langs.optional"))));
  }
  side.append(list);
}

// The file as the game reads it.
function localesRon(list) {
  return "// The languages of the game, in the order of the language list.\n"
    + "// `required: true`: a missing text stops `validate`. Without it a missing\n"
    + "// text is a warning and the game shows the English one. English is the\n"
    + "// source and always required. Managed in the editor (Languages).\n[\n"
    + list.map((l) => `    (code: ${JSON.stringify(l.code)}, required: ${l.code === "en" || l.required}),\n`).join("") + "]\n";
}

async function saveLangs() {
  // A new language gets an empty file beside each English one, so its
  // folder exists and the translation screen can fill it.
  if (!S.texts) await loadTexts();
  const names = [...new Set(S.texts.map((r) => r.file))];
  const fresh = G.list.filter((l) => !infoOf(l.code)?.folder)
    .flatMap((l) => names.map((n) => ({ path: `text/${l.code}/${n}`, text: `// Texts (${n.replace(/\.ron$/, "")}).\n{\n}\n` })));
  const r = await api("/api/save", { files: [{ path: "text/locales.ron", text: localesRon(G.list) }, ...fresh], texts: [] });
  if (!r.ok) { await say(t("editor.languages"), r.report.errors.join(" · ")); return; }
  S.meta = await api("/api/meta");
  await Promise.all([reloadContent(), loadLangs()]);
  toast(t("editor.langs.saved"));
  draw();
}

function addLang() {
  const code = el("input", { class: "mono", list: "known-codes", placeholder: "fr, pt-PT, pl…" });
  const req = el("input", { type: "checkbox" });
  const msg = el("p", { class: "hint" });
  const check = () => {
    const c = code.value.trim();
    const ok = CODE_RE.test(c) && !G.list.some((l) => l.code.toLowerCase() === c.toLowerCase());
    msg.className = "hint" + (c && !ok ? " warnline" : "");
    msg.textContent = !c ? t("editor.langs.code_hint")
      : !CODE_RE.test(c) ? t("editor.langs.bad_code")
      : !ok ? t("editor.langs.taken")
      : `${langName(c, c)} · ${KNOWN_CODES.some((k) => k === c || k === c.split("-")[0]) ? t("editor.langs.rules_ok") : t("editor.langs.rules_missing")}`;
    return ok;
  };
  code.addEventListener("input", check);
  check();
  return dialog(t("editor.langs.add"), [
    el("datalist", { id: "known-codes" }, KNOWN_CODES.filter((c) => !G.list.some((l) => l.code === c))
      .map((c) => el("option", { value: c }, langName(c, S.uiLocale)))),
    el("div", { class: "fld" }, el("label", {}, t("editor.langs.code")), code), msg,
    el("label", { class: "check" }, req, t("editor.langs.required")),
    el("p", { class: "hint" }, t("editor.langs.required_hint")),
  ], [
    { label: t("editor.cancel"), value: false },
    { label: t("editor.add"), value: true, primary: true },
  ], { validate: check }).then((ok) => {
    if (!ok) return;
    G.list.push({ code: code.value.trim(), required: req.checked });
    draw();
  });
}

function drawLangs() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page" });
  const dirtyNow = langsDirty();
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, "text/locales.ron"), el("h1", {}, t("editor.languages"))),
    el("div", { class: "grow" }),
    el("button", { class: "ghost", disabled: !dirtyNow, onclick: () => { G.list = JSON.parse(G.saved); draw(); } }, t("editor.looks.discard")),
    el("button", { class: "primary", disabled: !dirtyNow, onclick: saveLangs }, icon("check", 16), t("editor.save"))));
  page.append(el("p", { class: "hint" }, t("editor.langs.intro")));
  const rows = el("div", { class: "langs" });
  G.list.forEach((l, idx) => {
    const i = infoOf(l.code);
    const en = l.code === "en";
    const move = (d) => { const j = idx + d; if (j < 1 || j >= G.list.length || en) return; [G.list[idx], G.list[j]] = [G.list[j], G.list[idx]]; draw(); };
    const bar = (pair, label) => el("div", { class: "lbar" }, el("span", { class: "hint" }, label),
      el("div", { class: "bar" }, el("span", { style: `width:${pct(pair)}%` })),
      el("span", { class: "hint mono" }, pair[1] ? `${pair[0]}/${pair[1]}` : "—"));
    const rules = i?.rules
      ? el("span", { class: "hint" }, t("editor.langs.rules").replace("{plural}", i.rules.plural)
        .replace("{decimal}", i.rules.decimal).replace("{group}", i.rules.group === " " ? "␣" : i.rules.group))
      : i ? el("span", { class: "chip warn" }, t("editor.langs.rules_missing")) : el("span", { class: "hint" }, t("editor.langs.unsaved"));
    rows.append(el("section", { class: "card lang" },
      el("div", { class: "lang-head" },
        el("div", { class: "thumb letter", style: `--s:40px;--h:${hue(l.code)}` }, l.code.slice(0, 2).toUpperCase()),
        el("div", { class: "grow" }, el("strong", {}, langName(l.code, l.code)), " ", el("code", {}, l.code),
          el("div", {}, rules)),
        el("button", { class: "icon-btn", title: t("editor.langs.up"), disabled: en || idx <= 1, onclick: () => move(-1) }, "↑"),
        el("button", { class: "icon-btn", title: t("editor.langs.down"), disabled: en || idx === G.list.length - 1, onclick: () => move(1) }, "↓")),
      el("label", { class: "check" }, el("input", { type: "checkbox", checked: en || l.required, disabled: en,
        onchange: (e) => { l.required = e.target.checked; draw(); } }), t("editor.langs.required"),
        el("span", { class: "hint" }, " — " + t(en ? "editor.langs.source" : "editor.langs.required_hint"))),
      i ? bar(i.content, t("editor.langs.content")) : null,
      i ? bar(i.ui, t("editor.langs.ui")) : null,
      el("div", { class: "btnrow" },
        en || !i ? null : el("button", { class: "ghost sm", onclick: async () => { S.textLocale = l.code; await go("texts"); } }, t("editor.langs.translate")),
        en ? null : el("button", { class: "ghost sm danger", disabled: !!i?.folder,
          title: i?.folder ? t("editor.langs.remove_folder").replace("{code}", l.code) : "",
          onclick: () => { G.list.splice(idx, 1); draw(); } }, t("editor.langs.remove")))));
  });
  page.append(rows,
    el("button", { class: "ghost", onclick: addLang }, icon("plus", 16), t("editor.langs.add")),
    el("p", { class: "hint" }, t("editor.langs.ui_hint")));
  p.append(page);
}
