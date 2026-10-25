// Economy and Moderation screens: the `admin` tool in the page, over the
// game database the editor was started with (--db). Every action is the
// same transaction, with the same audit row, as from the terminal.
"use strict";

const AD = {
  status: null, economy: null, suspects: null, reasons: [],
  account: null, query: "", reports: null, log: null, guesses: null,
  suspectQ: { since: "24h", kills_per_hour: 300, max_session: "8h" }, reportsSince: "7d", logSince: "7d",
};

async function loadAdmin(view) {
  AD.status = await api("/api/admin/status");
  if (!AD.status.open) return;
  const safe = async (p) => { try { return await api(p); } catch (e) { return { error: e.message }; } };
  if (view === "economy") {
    [AD.economy, AD.suspects] = await Promise.all([safe("/api/admin/economy"), loadSuspects()]);
  } else {
    if (!AD.reasons.length) AD.reasons = await safe("/api/admin/reasons");
    [AD.reports, AD.log, AD.guesses] = await Promise.all([
      safe(`/api/admin/reports?since=${AD.reportsSince}`),
      safe(`/api/admin/log?since=${AD.logSince}`),
      safe("/api/admin/failed_logins"),
    ]);
    if (AD.query) await findAccount(AD.query, true);
  }
}

async function loadSuspects() {
  const q = new URLSearchParams(AD.suspectQ);
  try { return await api(`/api/admin/suspects?${q}`); } catch (e) { return { error: e.message }; }
}

const num = (n) => Number(n || 0).toLocaleString(S.uiLocale);
const when = (unix) => unix ? new Date(unix * 1000).toLocaleString(S.uiLocale) : "—";
const dur = (s) => s >= 3600 ? `${(s / 3600).toLocaleString(S.uiLocale, { maximumFractionDigits: 1 })} h` : `${Math.round(s / 60)} min`;

function noDb() {
  return el("section", { class: "card" },
    el("div", { class: "card-head" }, icon("alert", 16), el("h3", {}, t("editor.admin.no_db"))),
    el("p", { class: "hint warnline" }, AD.status.missing),
    el("p", { class: "hint" }, t("editor.admin.no_db_hint")),
    el("pre", { class: "mono hint" }, "editor --db sqlite:data/game.db"));
}

function adminPage(titleKey, crumb, refresh) {
  const page = el("div", { class: "page wide" });
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, crumb), el("h1", {}, t(titleKey))),
    el("div", { class: "grow" }),
    AD.status.open ? el("button", { class: "ghost", onclick: refresh }, t("editor.admin.refresh")) : null));
  return page;
}

function errCard(v) {
  return v && v.error ? el("p", { class: "hint warnline" }, v.error) : null;
}

// A list of name -> amount with a bar against the largest.
function bars(pairs, color) {
  const max = Math.max(1, ...pairs.map((p) => p[1]));
  const box = el("div", { class: "stat-bars" });
  for (const [name, n] of pairs) {
    box.append(el("div", { class: "stat-bar" },
      el("span", { class: "sb-name", title: name }, name),
      el("span", { class: "sb-track" }, el("span", { class: "sb-fill", style: `width:${(100 * n) / max}%;background:${color}` })),
      el("span", { class: "sb-n mono" }, num(n))));
  }
  if (!pairs.length) box.append(el("p", { class: "hint" }, t("editor.admin.nothing")));
  return box;
}

function tile(label, value, sub, cls) {
  return el("div", { class: "stat-tile" + (cls ? " " + cls : "") },
    el("div", { class: "st-label" }, label), el("div", { class: "st-value" }, value), sub ? el("div", { class: "hint" }, sub) : null);
}

// ---------- Economy ----------

function drawEconomySide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.economy"))));
  const list = el("div", { class: "list" });
  for (const [anchor, key] of [["eco-now", "editor.economy.now"], ["eco-flow", "editor.economy.flow"], ["eco-suspects", "editor.economy.suspects"]]) {
    list.append(el("div", { class: "entry", onclick: () => $(anchor)?.scrollIntoView({ behavior: "smooth" }) },
      el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(anchor)}` }, icon("chart", 18)),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, t(key)))));
  }
  side.append(list);
}

function drawEconomy() {
  const p = $("panel");
  p.replaceChildren();
  const page = adminPage("editor.economy", AD.status?.url || "", async () => { await loadAdmin("economy"); draw(); });
  p.append(page);
  if (!AD.status.open) { page.append(noDb()); return; }
  const e = AD.economy;
  if (!e || e.error) { page.append(errCard(e)); return; }
  const f = e.flow;
  const sink = f.sink_permille;
  page.append(el("p", { class: "hint" }, t("editor.economy.intro")));
  page.append(el("div", { class: "stat-tiles", id: "eco-now" },
    tile(t("editor.economy.characters"), num(e.characters)),
    tile(t("editor.economy.coins"), num(e.coins), e.characters ? fmt("editor.economy.per_character", { n: num(Math.round(e.coins / e.characters)) }) : null),
    tile(t("editor.economy.created"), num(f.created), t("editor.economy.last_week")),
    tile(t("editor.economy.destroyed"), num(f.destroyed), t("editor.economy.last_week")),
    tile(t("editor.economy.sink_ratio"), sink == null ? "—" : `${(sink / 10).toLocaleString(S.uiLocale, { maximumFractionDigits: 1 })}%`,
      t("editor.economy.sink_goal"), sink == null ? "" : sink >= 900 ? "ok" : "warn")));

  page.append(el("section", { class: "card", id: "eco-flow" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.economy.flow")), el("span", { class: "card-hint" }, t("editor.economy.last_week"))),
    el("div", { class: "two-col" },
      el("div", {}, el("h4", {}, t("editor.economy.sources")), bars(Object.entries(f.sources).sort((a, b) => b[1] - a[1]), "var(--ok)")),
      el("div", {}, el("h4", {}, t("editor.economy.sinks")), bars(Object.entries(f.sinks).sort((a, b) => b[1] - a[1]), "var(--warn)")))));

  page.append(el("section", { class: "card" },
    el("div", { class: "two-col" },
      el("div", {}, el("h4", {}, t("editor.economy.levels")), bars(e.levels.map(([l, n]) => [fmt("editor.economy.level", { n: l }), n]), "var(--accent)")),
      el("div", {}, el("h4", {}, t("editor.economy.richest")), bars(e.richest, "var(--accent-2)")))));

  const items = [...e.items].sort((a, b) => (b[1] + b[2]) - (a[1] + a[2]));
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.economy.items")), el("span", { class: "card-hint" }, t("editor.economy.items_hint"))),
    el("div", { class: "stat-table" },
      el("div", { class: "st-row st-head" }, el("span", {}, t("editor.economy.item")), el("span", {}, t("editor.economy.stacked")), el("span", {}, t("editor.economy.unique"))),
      items.map(([id, s, u]) => el("div", { class: "st-row" }, el("span", { class: "mono" }, id), el("span", { class: "mono" }, num(s)), el("span", { class: "mono" }, num(u)))))));

  // Bot telemetry: the limits are a filter, a human decides.
  const q = AD.suspectQ;
  const input = (k, w) => el("input", { value: q[k], style: `width:${w}px`, onchange: (ev) => { q[k] = ev.target.value; } });
  const s = AD.suspects;
  page.append(el("section", { class: "card", id: "eco-suspects" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.economy.suspects"))),
    el("p", { class: "hint" }, t("editor.economy.suspects_hint")),
    el("div", { class: "toolbar" },
      el("label", { class: "hint" }, t("editor.economy.since")), input("since", 70),
      el("label", { class: "hint" }, t("editor.economy.kills_per_hour")), input("kills_per_hour", 80),
      el("label", { class: "hint" }, t("editor.economy.max_session")), input("max_session", 70),
      el("button", { class: "ghost", onclick: async () => { AD.suspects = await loadSuspects(); draw(); } }, t("editor.admin.refresh"))),
    errCard(s),
    Array.isArray(s) ? (s.length ? el("div", { class: "stat-table" },
      el("div", { class: "st-row st-head" }, el("span", {}, t("editor.economy.character")), el("span", {}, t("editor.economy.kills_per_hour")),
        el("span", {}, t("editor.economy.longest_session")), el("span", {}, t("editor.economy.levels_gained"))),
      s.map((x) => el("div", { class: "st-row" },
        el("a", { href: "#", onclick: (ev) => { ev.preventDefault(); openModeration(x.character); } }, x.character),
        el("span", { class: "mono" }, num(x.kills_per_hour)), el("span", { class: "mono" }, dur(x.longest_session_secs)), el("span", { class: "mono" }, x.levels_gained))))
      : el("p", { class: "hint" }, t("editor.economy.no_suspects"))) : null));
}

// ---------- Moderation ----------

async function openModeration(name) {
  AD.query = name;
  if (S.view !== "moderation") await go("moderation");
  else { await findAccount(name); draw(); }
}

async function findAccount(name, quiet) {
  AD.query = name.trim();
  if (!AD.query) { AD.account = null; return; }
  try {
    AD.account = await api(`/api/admin/account?name=${encodeURIComponent(AD.query)}`);
  } catch (e) {
    AD.account = null;
    if (!quiet) toast(e.message, "err");
  }
}

function drawModerationSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.moderation"))));
  const list = el("div", { class: "list" });
  for (const [anchor, key, n] of [["mod-account", "editor.mod.account", null],
    ["mod-guesses", "editor.mod.guesses", Array.isArray(AD.guesses) ? AD.guesses.length : null],
    ["mod-reports", "editor.mod.reports", Array.isArray(AD.reports) ? AD.reports.length : null],
    ["mod-log", "editor.mod.log", null], ["mod-maintenance", "editor.mod.maintenance", null]]) {
    list.append(el("div", { class: "entry", onclick: () => $(anchor)?.scrollIntoView({ behavior: "smooth" }) },
      el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(anchor)}` }, icon("shield", 18)),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, t(key))),
      n ? el("span", { class: "chip warn" }, n) : null));
  }
  side.append(list);
}

// Runs an action; a reset password is shown once, in a dialog.
async function act(body) {
  try {
    const r = await api("/api/admin/action", body);
    if (body.action === "reset_password") {
      await dialog(t("editor.mod.reset_password"), [el("p", {}, t("editor.mod.password_once")), el("pre", { class: "mono big-code" }, r.message)],
        [{ label: t("editor.ok"), value: true, primary: true }]);
    } else toast(r.message);
  } catch (e) {
    await say(t("editor.moderation"), e.message);
    return false;
  }
  await loadAdmin("moderation");
  draw();
  return true;
}

const DURATIONS = ["1h", "1d", "3d", "7d", "30d"];

// Duration (for timed actions), reason and note, then the action.
async function sanctionDialog(action, target) {
  const timed = action !== "ban";
  let duration = action === "mute" ? "1h" : action === "ip_ban" ? "7d" : "3d";
  const custom = el("input", { value: duration, style: "width:90px", oninput: (e) => { duration = e.target.value; mark(); } });
  const seg = el("div", { class: "seg" });
  const mark = () => { for (const b of seg.children) b.classList.toggle("on", b.textContent === duration); };
  for (const d of DURATIONS) seg.append(el("button", { onclick: () => { duration = d; custom.value = d; mark(); } }, d));
  mark();
  const reason = el("select", {}, (Array.isArray(AD.reasons) ? AD.reasons : []).map((r) => el("option", { value: r.key }, r.text)));
  const note = el("textarea", { rows: 3, placeholder: t("editor.mod.note_hint") });
  const body = [
    el("p", {}, fmt("editor.mod.confirm_" + action, { target })),
    timed ? el("div", { class: "f" }, el("label", {}, t("editor.mod.duration")), el("div", { class: "toolbar" }, seg, custom)) : null,
    el("div", { class: "f" }, el("label", {}, t("editor.mod.reason")), reason),
    el("div", { class: "f" }, el("label", {}, t("editor.mod.note")), note),
  ];
  const ok = await dialog(t("editor.mod.action_" + action), body.filter(Boolean),
    [{ label: t("editor.cancel"), value: false }, { label: t("editor.mod.action_" + action), value: true, primary: true }]);
  if (!ok) return;
  const b = { action, reason: reason.value, note: note.value, duration };
  if (action === "ip_ban") b.ip = target; else b.account = target;
  await act(b);
}

async function confirmAct(titleKey, text, body) {
  const ok = await dialog(t(titleKey), [el("p", {}, text)],
    [{ label: t("editor.cancel"), value: false }, { label: t(titleKey), value: true, primary: true }]);
  if (ok) await act(body);
}

async function renameDialog(character) {
  const input = el("input", { value: character, autofocus: true });
  const ok = await dialog(t("editor.mod.rename"), [el("p", {}, t("editor.mod.rename_hint")), input],
    [{ label: t("editor.cancel"), value: false }, { label: t("editor.mod.rename"), value: true, primary: true }]);
  if (ok && input.value.trim() && input.value.trim() !== character) await act({ action: "rename", character, new_name: input.value.trim() });
}

function sanctionRow(s) {
  const state = s.revoked_at ? el("span", { class: "chip" }, t("editor.mod.revoked"))
    : s.active ? el("span", { class: "chip err" }, t("editor.mod.active")) : el("span", { class: "chip" }, t("editor.mod.expired"));
  const reason = (Array.isArray(AD.reasons) ? AD.reasons : []).find((r) => r.key === s.reason)?.text || s.reason;
  return el("div", { class: "st-row sanction" },
    el("span", { class: "mono" }, `#${s.id}`),
    el("span", {}, el("b", {}, t("editor.mod.kind." + s.kind)), " · ", reason, s.note ? el("div", { class: "hint" }, s.note) : null),
    el("span", { class: "hint" }, when(s.created_at), " → ", s.expires_at ? when(s.expires_at) : t("editor.mod.permanent"),
      el("div", {}, s.created_by)),
    el("span", {}, state, s.active && !s.revoked_at ? el("button", { class: "ghost small", onclick: () => confirmAct("editor.mod.revoke", fmt("editor.mod.confirm_revoke", { id: s.id }), { action: "revoke", id: s.id }) }, t("editor.mod.revoke")) : null));
}

function accountCard() {
  const a = AD.account;
  const search = el("input", { type: "search", value: AD.query, placeholder: t("editor.mod.search_hint"), autofocus: true });
  const run = async () => { await findAccount(search.value); draw(); };
  search.addEventListener("keydown", (e) => { if (e.key === "Enter") run(); });
  const card = el("section", { class: "card", id: "mod-account" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.mod.account"))),
    el("div", { class: "toolbar" }, el("label", { class: "search grow" }, icon("search", 16), search),
      el("button", { class: "primary", onclick: run }, t("editor.mod.find"))));
  if (!a) {
    card.append(el("p", { class: "hint" }, AD.query ? t("editor.mod.not_found") : t("editor.mod.search_intro")));
    return card;
  }
  const admin = a.role === "admin";
  card.append(
    el("div", { class: "acct-head" },
      el("div", { class: "thumb letter", style: `--s:44px;--h:${hue(a.name)}` }, a.name.slice(0, 1).toUpperCase()),
      el("div", {}, el("div", { class: "acct-name" }, a.name, " ", el("span", { class: "chip" + (admin ? " ok" : "") }, a.role)),
        el("div", { class: "hint" }, `id ${a.id} · ${a.locale}`, a.must_change_password ? " · " + t("editor.mod.must_change") : ""))),
    el("div", { class: "toolbar" },
      el("button", { class: "ghost", onclick: () => sanctionDialog("mute", a.name) }, t("editor.mod.action_mute")),
      el("button", { class: "ghost", onclick: () => sanctionDialog("suspend", a.name) }, t("editor.mod.action_suspend")),
      el("button", { class: "danger", onclick: () => sanctionDialog("ban", a.name) }, t("editor.mod.action_ban")),
      el("button", { class: "ghost", onclick: () => confirmAct("editor.mod.reset_password", fmt("editor.mod.confirm_reset", { target: a.name }), { action: "reset_password", account: a.name }) }, t("editor.mod.reset_password")),
      el("button", { class: "ghost", onclick: () => confirmAct(admin ? "editor.mod.demote" : "editor.mod.promote", fmt(admin ? "editor.mod.confirm_demote" : "editor.mod.confirm_promote", { target: a.name }), { action: admin ? "demote" : "promote", account: a.name }) }, t(admin ? "editor.mod.demote" : "editor.mod.promote"))),
    el("h4", {}, t("editor.mod.characters")),
    el("div", { class: "stat-table" }, a.characters.length ? a.characters.map((c) => el("div", { class: "st-row" },
      el("span", {}, c.name), el("span", { class: "hint" }, fmt("editor.economy.level", { n: c.level })),
      el("span", { class: "hint mono" }, `id ${c.id}`),
      el("span", {}, el("button", { class: "ghost small", onclick: () => renameDialog(c.name) }, t("editor.mod.rename")))))
      : el("p", { class: "hint" }, t("editor.admin.nothing"))),
    ...(a.deleted?.length ? [el("h4", {}, t("editor.mod.deleted_characters")), el("div", { class: "stat-table" }, a.deleted.map((c) => el("div", { class: "st-row" },
      el("span", {}, c.name), el("span", { class: "hint" }, when(c.at)),
      el("span", { class: "hint mono" }, `id ${c.id}`),
      el("span", {}, el("button", { class: "ghost small", onclick: () => confirmAct("editor.mod.restore", fmt("editor.mod.confirm_restore", { target: c.name }), { action: "restore", id: c.id }) }, t("editor.mod.restore"))))))] : []),
    el("h4", {}, t("editor.mod.sanctions")),
    el("div", { class: "stat-table" }, a.sanctions.length ? [...a.sanctions].reverse().map(sanctionRow) : el("p", { class: "hint" }, t("editor.admin.nothing"))),
    el("h4", {}, t("editor.mod.logins")),
    el("div", { class: "stat-table" }, a.logins.length ? a.logins.map((l) => el("div", { class: "st-row" },
      el("span", { class: "mono" }, l.ip), el("span", { class: "hint" }, when(l.at)),
      el("span", {}, el("button", { class: "ghost small", onclick: () => sanctionDialog("ip_ban", l.ip) }, t("editor.mod.action_ip_ban")))))
      : el("p", { class: "hint" }, t("editor.admin.nothing"))));
  return card;
}

function sinceSeg(value, onpick) {
  return el("div", { class: "seg" }, ["1d", "7d", "30d"].map((d) => el("button", { class: d === value ? "on" : "", onclick: () => onpick(d) }, d)));
}

function drawModeration() {
  const p = $("panel");
  p.replaceChildren();
  const page = adminPage("editor.moderation", AD.status?.url || "", async () => { await loadAdmin("moderation"); draw(); });
  p.append(page);
  if (!AD.status.open) { page.append(noDb()); return; }
  page.append(el("p", { class: "hint" }, t("editor.mod.intro")), accountCard());

  // Accounts someone is guessing the password of.
  const g = AD.guesses;
  page.append(el("section", { class: "card", id: "mod-guesses" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.mod.guesses"))),
    el("p", { class: "hint" }, t("editor.mod.guesses_hint")),
    errCard(g),
    Array.isArray(g) ? (g.length ? el("div", { class: "stat-table" }, g.map((x) => el("div", { class: "st-row" },
      el("a", { href: "#", onclick: (e) => { e.preventDefault(); openModeration(x.account); } }, x.account),
      el("span", { class: "chip warn" }, num(x.failed_logins)))))
      : el("p", { class: "hint" }, t("editor.mod.no_guesses"))) : null));

  const r = AD.reports;
  page.append(el("section", { class: "card", id: "mod-reports" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.mod.reports")),
      el("span", { class: "card-hint" }, sinceSeg(AD.reportsSince, async (d) => { AD.reportsSince = d; await loadAdmin("moderation"); draw(); }))),
    errCard(r),
    Array.isArray(r) ? (r.length ? r.map((x) => el("div", { class: "report" },
      el("div", { class: "report-head" }, el("b", {}, x.reporter), " → ",
        el("a", { href: "#", onclick: (e) => { e.preventDefault(); openModeration(x.target); } }, x.target),
        el("span", { class: "hint" }, " · ", when(x.created_at), " · #", x.id)),
      el("div", { class: "report-lines" }, x.lines.map((l) => el("div", { class: "rl" + (l.from === x.target ? " target" : "") },
        el("span", { class: "hint mono" }, new Date(l.at * 1000).toLocaleTimeString(S.uiLocale)), " ",
        el("b", {}, l.from), l.channel !== "local" ? el("span", { class: "tag" }, l.channel) : null, " ", l.text)))))
      : el("p", { class: "hint" }, t("editor.mod.no_reports"))) : null));

  const lg = AD.log;
  page.append(el("section", { class: "card", id: "mod-log" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.mod.log")),
      el("span", { class: "card-hint" }, sinceSeg(AD.logSince, async (d) => { AD.logSince = d; await loadAdmin("moderation"); draw(); }))),
    errCard(lg),
    Array.isArray(lg) ? (lg.length ? el("div", { class: "stat-table" }, lg.map((x) => el("div", { class: "st-row log" },
      el("span", { class: "hint" }, when(x.at)), el("span", { class: "mono" }, x.actor), el("span", {}, el("b", {}, x.action), " ", x.target),
      el("span", { class: "hint" }, [x.args, x.note].filter(Boolean).join(" · ")))))
      : el("p", { class: "hint" }, t("editor.admin.nothing"))) : null));

  // Announced restart: the server reads it within 30 s.
  let wait = "10m";
  const input = el("input", { value: wait, style: "width:90px", oninput: (e) => { wait = e.target.value; } });
  page.append(el("section", { class: "card", id: "mod-maintenance" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.mod.maintenance"))),
    el("p", { class: "hint" }, t("editor.mod.maintenance_hint")),
    el("div", { class: "toolbar" }, input,
      el("button", { class: "danger", onclick: () => confirmAct("editor.mod.announce", fmt("editor.mod.confirm_maintenance", { wait }), { action: "maintenance", duration: wait }) }, t("editor.mod.announce")),
      el("button", { class: "ghost", onclick: () => confirmAct("editor.mod.cancel_maintenance", t("editor.mod.confirm_cancel_maintenance"), { action: "maintenance", duration: "cancel" }) }, t("editor.mod.cancel_maintenance")))));
}
