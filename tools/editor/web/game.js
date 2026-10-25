// Game screen: the game's name in every language (`ui.game.title` in the
// client's text files) and the picture behind the login screens
// (assets/ui/login_background.png).
"use strict";

const GM = { info: null, names: {}, saved: "", stamp: Date.now() };

async function loadGame() {
  GM.info = await api("/api/game");
  GM.names = { ...GM.info.names };
  GM.saved = JSON.stringify(GM.names);
}

function gameDirty() { return !!GM.info && JSON.stringify(GM.names) !== GM.saved; }

function drawGameSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.game"))));
  const list = el("div", { class: "list" });
  const entry = (anchor, ic, title, sub) => list.append(el("div", { class: "entry", onclick: () => $(anchor)?.scrollIntoView({ behavior: "smooth" }) },
    el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(anchor)}` }, icon(ic, 18)),
    el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, title), el("div", { class: "hint" }, sub))));
  entry("game-name", "flag", t("editor.game.name"), GM.names.en || "—");
  const bg = GM.info?.background;
  entry("game-bg", "image", t("editor.game.background"), bg ? `${bg.width}×${bg.height}` : t("editor.game.default_background"));
  side.append(list);
}

async function saveGame() {
  try {
    GM.info = await api("/api/game", { names: GM.names });
  } catch (e) {
    await say(t("editor.game"), e.message);
    return;
  }
  GM.names = { ...GM.info.names };
  GM.saved = JSON.stringify(GM.names);
  toast(t("editor.game.saved"));
  draw();
}

async function uploadBackground(file) {
  if (!file) return;
  try {
    await api("/api/game/background", file, true);
  } catch (e) {
    await say(t("editor.game.background"), e.message);
    return;
  }
  GM.info = await api("/api/game");
  GM.stamp = Date.now();
  toast(t("editor.game.background_saved"));
  draw();
}

function drawGame() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page" });
  const dirtyNow = gameDirty();
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, "assets/text/*/ui.ron · assets/ui/"), el("h1", {}, t("editor.game"))),
    el("div", { class: "grow" }),
    el("button", { class: "ghost", disabled: !dirtyNow, onclick: () => { GM.names = JSON.parse(GM.saved); draw(); } }, t("editor.looks.discard")),
    el("button", { class: "primary", disabled: !dirtyNow, onclick: saveGame }, icon("check", 16), t("editor.save"))));
  page.append(el("p", { class: "hint" }, t("editor.game.intro")));

  // The name, English first: the other languages may keep it or write it
  // in their own script.
  const max = GM.info.max_chars;
  const grid = el("div", { class: "fields" });
  for (const l of GM.info.locales) {
    const v = GM.names[l] ?? "";
    const over = max && [...v].length > max;
    const input = el("input", {
      value: v, maxlength: max || undefined, class: over || !v.trim() ? "bad" : "",
      oninput: (e) => { GM.names[l] = e.target.value; drawGameHead(); },
    });
    grid.append(el("div", { class: "f" }, el("label", {}, `${langName(l, S.uiLocale)} · ${l}`), input));
  }
  page.append(el("section", { class: "card", id: "game-name" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.game.name")),
      max ? el("span", { class: "card-hint" }, fmt("editor.game.name_limit", { n: max })) : null),
    el("p", { class: "hint" }, t("editor.game.name_hint")), grid));

  // The background, with the name over it as the login shows it.
  const bg = GM.info.background;
  const file = el("input", { type: "file", accept: "image/png", hidden: true, onchange: (e) => uploadBackground(e.target.files[0]) });
  const preview = el("div", { class: "login-preview" + (bg ? "" : " circle") },
    bg ? el("img", { src: `/api/game/background?v=${GM.stamp}`, alt: "" }) : null,
    el("div", { class: "login-name" }, GM.names.en || ""));
  page.append(el("section", { class: "card", id: "game-bg" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.game.background")),
      el("span", { class: "card-hint" }, bg ? `${bg.width}×${bg.height} · ${Math.round(bg.bytes / 1024)} KiB` : t("editor.game.default_background"))),
    el("p", { class: "hint" }, t("editor.game.background_hint")),
    preview,
    el("div", { class: "toolbar", style: "margin-top:12px" },
      el("button", { class: "primary", onclick: () => file.click() }, icon("upload", 16), t(bg ? "editor.game.replace_background" : "editor.game.upload_background")),
      file)));
  p.append(page);
}

// Only the save buttons follow the typing (redrawing would lose the focus).
function drawGameHead() {
  const dirtyNow = gameDirty();
  for (const b of $("panel").querySelectorAll(".page-head button")) b.disabled = !dirtyNow;
}
