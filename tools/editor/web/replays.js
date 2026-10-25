// Replays screen: the logs a zone recorded (replays/, on a panic or by the
// admin `/replay` command), and one of them run again by the `replay`
// library and drawn from above: ground, players, monsters, NPCs and nodes,
// tick by tick, with a timeline.
"use strict";

const RP = {
  list: null, file: null, run: null, loading: false, error: "",
  frame: 0, playing: false, speed: 1, timer: 0, hover: null, follow: null,
  // Entities after applying frames 0..=frame (rebuilt on a jump back).
  state: new Map(), at: -1, ground: null, colors: {},
};

async function loadReplays() {
  RP.list = await api("/api/replays");
  if (!M.tiles) M.tiles = await api("/api/tiles");
}

function replaysDirty() { return false; }

function stopPlay() { RP.playing = false; clearInterval(RP.timer); RP.timer = 0; }

function drawReplaysSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.replays")),
    el("div", { class: "hint mono" }, RP.list?.dir || "")));
  const list = el("div", { class: "list" });
  for (const f of RP.list?.files || []) {
    const sub = f.error ? f.error
      : `${f.zone} · ${fmt("editor.replays.ticks", { n: num(f.records) })} · ${(f.records / 20 / 60).toLocaleString(S.uiLocale, { maximumFractionDigits: 1 })} min`;
    list.append(el("div", { class: "entry" + (RP.file === f.file ? " on" : ""), onclick: () => openReplay(f.file) },
      el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(f.zone || f.file)}` }, icon("play", 18)),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, when(f.at)), el("div", { class: "hint" }, sub)),
      f.same_content === false ? el("span", { class: "chip warn", title: t("editor.replays.other_content") }, "≠") : null));
  }
  if (!(RP.list?.files || []).length) list.append(el("p", { class: "hint", style: "padding:12px" }, t("editor.replays.none")));
  side.append(list);
}

async function openReplay(file) {
  stopPlay();
  RP.file = file; RP.run = null; RP.error = ""; RP.loading = true; RP.frame = 0; RP.state = new Map(); RP.at = -1; RP.ground = null;
  draw();
  try {
    RP.run = await api(`/api/replays/run?file=${encodeURIComponent(file)}&every=2`);
    await paintGround();
  } catch (e) {
    RP.error = e.message;
  }
  RP.loading = false;
  draw();
}

// The ground once, in an offscreen canvas: one pixel per tile, scaled up.
async function paintGround() {
  const m = RP.run.map;
  const used = [...new Set(m.floors)].filter((f) => f);
  await Promise.all(used.map((f) => tileColor(f)));
  const c = document.createElement("canvas");
  c.width = m.w; c.height = m.h;
  const g = c.getContext("2d");
  for (let y = 0; y < m.h; y++) {
    for (let x = 0; x < m.w; x++) {
      const i = y * m.w + x;
      const f = m.floors[i];
      g.fillStyle = f ? M.colors[f] : "#05070a";
      g.fillRect(x, y, 1, 1);
      if (m.blocked[i] === "1" && f) { g.fillStyle = "rgba(0,0,0,.45)"; g.fillRect(x, y, 1, 1); }
    }
  }
  RP.ground = c;
}

// Brings RP.state to frame `n` (forward from where it is, or from 0).
function seek(n) {
  const frames = RP.run.frames;
  n = Math.max(0, Math.min(frames.length - 1, n));
  if (n < RP.at) { RP.state = new Map(); RP.at = -1; }
  for (let i = RP.at + 1; i <= n; i++) {
    const fr = frames[i];
    for (const [id, x, y, hp] of fr.set) RP.state.set(id, { x, y, hp });
    for (const id of fr.gone) RP.state.delete(id);
  }
  RP.at = n;
  RP.frame = n;
}

const KIND_COLOR = { player: "#3fb8ff", monster: "#ff6b6b", npc: "#f5b04c", node: "#4fd18b" };

function paintReplay(canvas) {
  const r = RP.run;
  const m = r.map;
  const g = canvas.getContext("2d");
  const k = Math.max(2, Math.floor(Math.min(canvas.width / m.w, canvas.height / m.h)));
  const ox = Math.floor((canvas.width - m.w * k) / 2), oy = Math.floor((canvas.height - m.h * k) / 2);
  canvas._view = { k, ox, oy };
  g.imageSmoothingEnabled = false;
  g.fillStyle = "#05070a";
  g.fillRect(0, 0, canvas.width, canvas.height);
  if (RP.ground) g.drawImage(RP.ground, ox, oy, m.w * k, m.h * k);
  // Nodes and NPCs under, monsters, players on top.
  const order = { node: 0, npc: 1, monster: 2, player: 3 };
  const list = [...RP.state.entries()].map(([id, s]) => ({ id, ...s, info: r.names[id] || { kind: "monster", name: "?" } }))
    .sort((a, b) => order[a.info.kind] - order[b.info.kind]);
  for (const e of list) {
    const cx = ox + (e.x - m.x + 0.5) * k, cy = oy + (e.y - m.y + 0.5) * k;
    const rad = Math.max(2, k * (e.info.kind === "player" ? 0.75 : e.info.kind === "node" ? 0.4 : 0.55));
    g.beginPath();
    g.arc(cx, cy, rad, 0, Math.PI * 2);
    g.fillStyle = KIND_COLOR[e.info.kind] || "#fff";
    g.globalAlpha = e.info.kind !== "node" && e.hp === 0 ? 0.35 : 1;
    g.fill();
    g.globalAlpha = 1;
    if (e.id === RP.hover || e.id === RP.follow) { g.strokeStyle = "#fff"; g.lineWidth = 2; g.stroke(); }
    if (e.info.kind === "player") {
      g.font = `${Math.max(10, k * 1.2)}px system-ui`;
      g.fillStyle = "#fff";
      g.textAlign = "center";
      g.fillText(e.info.name, cx, cy - rad - 3);
    }
  }
}

function play() {
  if (RP.playing) { stopPlay(); draw(); return; }
  if (RP.frame >= RP.run.frames.length - 1) seek(0);
  RP.playing = true;
  // A frame is `every` ticks; the server ticks at 20 Hz.
  const ms = (1000 / 20) * RP.run.every / RP.speed;
  RP.timer = setInterval(() => {
    if (S.view !== "replays" || !RP.run) { stopPlay(); return; }
    if (RP.frame >= RP.run.frames.length - 1) { stopPlay(); draw(); return; }
    seek(RP.frame + 1);
    refreshReplay();
  }, ms);
  draw();
}

// Redraws only the canvas and the timeline (not the whole page).
function refreshReplay() {
  const c = $("rp-canvas");
  if (!c) return;
  paintReplay(c);
  const fr = RP.run.frames[RP.frame];
  const slider = $("rp-slider");
  if (slider) slider.value = RP.frame;
  const lbl = $("rp-tick");
  if (lbl) lbl.textContent = `${fmt("editor.replays.tick", { n: num(fr.tick) })} · ${clock(fr.tick - RP.run.first_tick)}`;
  const cmds = $("rp-cmds");
  if (cmds) cmds.replaceChildren(...(fr.cmds.length ? fr.cmds.map(([n, c]) => el("span", { class: "tag" }, `${n} ×${c}`)) : [el("span", { class: "hint" }, "—")]));
  const counts = { player: 0, monster: 0, npc: 0, node: 0 };
  for (const id of RP.state.keys()) counts[RP.run.names[id]?.kind] = (counts[RP.run.names[id]?.kind] || 0) + 1;
  const cnt = $("rp-counts");
  if (cnt) cnt.replaceChildren(...Object.entries(counts).map(([k, n]) => el("span", { class: "legend-dot" },
    el("i", { style: `background:${KIND_COLOR[k]}` }), `${t("editor.replays.kind." + k)} ${n}`)));
  hoverInfo();
}

const clock = (ticks) => { const s = Math.floor(ticks / 20); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`; };

function hoverInfo() {
  const box = $("rp-hover");
  if (!box) return;
  const id = RP.hover ?? RP.follow;
  const s = id != null ? RP.state.get(id) : null;
  const n = id != null ? RP.run.names[id] : null;
  box.replaceChildren(s && n
    ? el("span", {}, el("b", {}, n.name), ` · ${t("editor.replays.kind." + n.kind)} · ${fmt("editor.economy.level", { n: n.level })} · (${s.x}, ${s.y}) · ${t("editor.replays.health")} ${s.hp}%`)
    : el("span", { class: "hint" }, t("editor.replays.hover_hint")));
}

function pickAt(canvas, ev) {
  const v = canvas._view;
  if (!v) return null;
  const rect = canvas.getBoundingClientRect();
  const sx = (ev.clientX - rect.left) * (canvas.width / rect.width), sy = (ev.clientY - rect.top) * (canvas.height / rect.height);
  const m = RP.run.map;
  const tx = (sx - v.ox) / v.k + m.x - 0.5, ty = (sy - v.oy) / v.k + m.y - 0.5;
  let best = null, bd = 2.5;
  for (const [id, s] of RP.state) {
    const d = Math.hypot(s.x - tx, s.y - ty);
    if (d < bd) { bd = d; best = id; }
  }
  return best;
}

function drawReplays() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page wide" });
  p.append(page);
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, RP.file || "replays/"), el("h1", {}, t("editor.replays"))),
    el("div", { class: "grow" }),
    el("button", { class: "ghost", onclick: async () => { await loadReplays(); draw(); } }, t("editor.admin.refresh"))));
  if (!RP.file) { page.append(el("p", { class: "hint" }, t("editor.replays.intro"))); return; }
  if (RP.loading) { page.append(el("p", { class: "hint" }, t("editor.replays.running"))); return; }
  if (RP.error) {
    page.append(el("section", { class: "card" }, el("div", { class: "card-head" }, icon("alert", 16), el("h3", {}, t("editor.replays.cannot"))),
      el("p", { class: "hint warnline" }, RP.error), el("p", { class: "hint" }, t("editor.replays.cannot_hint"))));
    return;
  }
  const r = RP.run;
  if (RP.at < 0) seek(0);
  page.append(el("div", { class: "stat-tiles" },
    tile(t("editor.replays.zone"), r.zone),
    tile(t("editor.replays.length"), clock(r.ticks), fmt("editor.replays.ticks", { n: num(r.ticks) })),
    tile(t("editor.replays.tick_cost"), `${num(r.mean_us)} µs`, fmt("editor.replays.worst", { n: num(r.worst_us) })),
    r.diverged ? tile(t("editor.replays.check"), t("editor.replays.diverged"), fmt("editor.replays.diverged_at", { n: num(r.diverged.tick) }), "warn")
      : tile(t("editor.replays.check"), t("editor.replays.identical"), t("editor.replays.identical_hint"), "ok")));

  const canvas = el("canvas", { id: "rp-canvas", class: "rp-canvas", width: 960, height: 600 });
  canvas.addEventListener("mousemove", (e) => { const id = pickAt(canvas, e); if (id !== RP.hover) { RP.hover = id; paintReplay(canvas); hoverInfo(); } });
  canvas.addEventListener("mouseleave", () => { RP.hover = null; paintReplay(canvas); hoverInfo(); });
  canvas.addEventListener("click", (e) => { RP.follow = pickAt(canvas, e); paintReplay(canvas); hoverInfo(); });
  const slider = el("input", { id: "rp-slider", type: "range", min: 0, max: r.frames.length - 1, value: RP.frame, class: "grow",
    oninput: (e) => { seek(Number(e.target.value)); refreshReplay(); } });
  const speed = el("div", { class: "seg" }, [0.5, 1, 2, 4, 8].map((s) => el("button", { class: s === RP.speed ? "on" : "",
    onclick: () => { RP.speed = s; if (RP.playing) { stopPlay(); play(); } else draw(); } }, `${s}×`)));
  page.append(el("section", { class: "card rp-card" },
    el("div", { class: "rp-legend", id: "rp-counts" }),
    canvas,
    el("div", { class: "toolbar rp-controls" },
      el("button", { class: "primary", onclick: play }, RP.playing ? "❚❚" : "▶"),
      el("button", { class: "ghost", onclick: () => { stopPlay(); seek(RP.frame - 10); refreshReplay(); } }, "−10"),
      el("button", { class: "ghost", onclick: () => { stopPlay(); seek(RP.frame + 10); refreshReplay(); } }, "+10"),
      slider, speed),
    el("div", { class: "rp-status" }, el("span", { id: "rp-tick", class: "mono" }), el("span", { id: "rp-hover" })),
    el("div", { class: "rp-cmds" }, el("span", { class: "hint" }, t("editor.replays.commands"), " "), el("span", { id: "rp-cmds" }))));

  const totals = Object.entries(r.commands).sort((a, b) => b[1] - a[1]);
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.replays.all_commands")), el("span", { class: "card-hint" }, fmt("editor.replays.seed", { n: r.seed }))),
    bars(totals, "var(--accent)")));
  requestAnimationFrame(refreshReplay);
}
