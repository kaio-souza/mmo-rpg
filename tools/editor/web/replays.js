// Replays screen: the logs a zone recorded (replays/, on a panic or by the
// admin `/replay` command), and one of them run again by the `replay`
// library and drawn as the game shows it (isometric, with the real tile and
// creature sprites), tick by tick, with a timeline.
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

// The scene as the game shows it: isometric 64x32 tiles with their
// sprites, walls and creatures sorted by depth, creatures with their
// strips facing where they look, moving smoothly between recorded frames.
const RP_TW = 32, RP_TH = 16;
const rpIso = (x, y) => [(x - y) * RP_TW, (x + y) * RP_TH];

async function paintGround() {
  // Sprites and their layouts come from the art and the manifest.
  if (!S.art) await loadArt();
  if (!L.manifest) L.manifest = await api("/api/sprite-manifest");
  const m = RP.run.map;
  const used = [...new Set(m.floors)].filter((f) => f);
  await Promise.all(used.map((f) => tileColor(f)));
  RP.ground = true;
  RP.cam = null;
}

// An image by sprite name (loaded once), and a tinted copy (body by skin,
// hair by its colour), as the client multiplies light grey art.
function rpImg(name) {
  RP.imgs = RP.imgs || new Map();
  let im = RP.imgs.get(name);
  if (!im) { im = new Image(); im.src = artUrl("sprite", name); RP.imgs.set(name, im); }
  return im;
}
function rpTinted(name, tint) {
  const im = rpImg(name);
  if (!tint || !im.complete || !im.naturalWidth) return im;
  RP.tints = RP.tints || new Map();
  const key = name + tint;
  let c = RP.tints.get(key);
  if (!c) {
    c = document.createElement("canvas");
    c.width = im.naturalWidth; c.height = im.naturalHeight;
    const g = c.getContext("2d");
    g.drawImage(im, 0, 0);
    g.globalCompositeOperation = "multiply";
    g.fillStyle = tint; g.fillRect(0, 0, c.width, c.height);
    g.globalCompositeOperation = "destination-in";
    g.drawImage(im, 0, 0);
    RP.tints.set(key, c);
  }
  return c;
}

// One sprite standing on screen point (sx, sy): its strip's frame and row.
function rpSprite(g, name, tint, sx, sy, k, row, moving, alpha) {
  const im = rpImg(name);
  if (!im.complete || !im.naturalWidth) return false;
  const l = layoutOf(name);
  if (!l.w) return false;
  const playing = l.frames > 1 && (l.play === "Always" || moving);
  const f = playing ? Math.floor((performance.now() / 1000) * l.fps) % l.frames : 0;
  const r = l.directions === 4 ? row : 0;
  g.globalAlpha = alpha;
  g.drawImage(rpTinted(name, tint), f * l.w, r * l.h, l.w, l.h, sx - l.anchor[0] * k, sy - l.anchor[1] * k, l.w * k, l.h * k);
  g.globalAlpha = 1;
  return true;
}

// Brings RP.state to frame `n` (forward from where it is, or from 0).
function seek(n) {
  const frames = RP.run.frames;
  n = Math.max(0, Math.min(frames.length - 1, n));
  if (n < RP.at) { RP.state = new Map(); RP.at = -1; }
  for (let i = RP.at + 1; i <= n; i++) {
    const fr = frames[i];
    for (const [id, x, y, hp, dir] of fr.set) {
      const old = RP.state.get(id);
      const moved = old && (old.x !== x || old.y !== y);
      RP.state.set(id, { x, y, hp, dir: dir ?? 0, px: moved ? old.x : (old?.px ?? x), py: moved ? old.y : (old?.py ?? y),
        at: moved ? performance.now() : (old?.at ?? 0) });
    }
    for (const id of fr.gone) RP.state.delete(id);
  }
  RP.at = n;
  RP.frame = n;
}

const KIND_COLOR = { player: "#3fb8ff", monster: "#ff6b6b", npc: "#f5b04c", node: "#4fd18b", companion: "#a78bfa" };

// Where an entity stands now: between its last two tiles while it walks.
function rpPos(e) {
  const ms = (1000 / 20) * RP.run.every / RP.speed;
  const t = e.at ? Math.min(1, (performance.now() - e.at) / ms) : 1;
  return [e.px + (e.x - e.px) * t, e.py + (e.y - e.py) * t, t < 1];
}

function paintReplay(canvas) {
  const r = RP.run;
  const m = r.map;
  const g = canvas.getContext("2d");
  const W = canvas.width, H = canvas.height;
  // The camera: fits the zone at first, follows the chosen entity.
  if (!RP.cam) {
    const [ax] = rpIso(m.x, m.y + m.h), [bx] = rpIso(m.x + m.w, m.y);
    const [, ay] = rpIso(m.x, m.y), [, by] = rpIso(m.x + m.w, m.y + m.h);
    const k = Math.max(0.25, Math.min(W / (bx - ax + 64), H / (by - ay + 96)));
    RP.cam = { k, cx: (ax + bx) / 2, cy: (ay + by) / 2 };
  }
  const fol = RP.follow != null && RP.state.get(RP.follow);
  if (fol) { const [fx, fy] = rpPos(fol); [RP.cam.cx, RP.cam.cy] = rpIso(fx, fy); }
  const { k, cx, cy } = RP.cam;
  const scr = (x, y) => { const [ix, iy] = rpIso(x, y); return [W / 2 + (ix - cx) * k, H / 2 + (iy - cy) * k]; };
  canvas._view = { scr, k };
  g.imageSmoothingEnabled = false;
  g.fillStyle = "#05070a";
  g.fillRect(0, 0, W, H);
  // Flat ground first; walls and creatures wait for the depth-sorted pass.
  const tall = [];
  for (let ty = 0; ty < m.h; ty++) {
    for (let tx = 0; tx < m.w; tx++) {
      const f = m.floors[ty * m.w + tx];
      if (!f) continue;
      const x = m.x + tx, y = m.y + ty;
      const [sx, sy] = scr(x, y);
      if (sx < -64 * k || sx > W + 64 * k || sy < -96 * k || sy > H + 64 * k) continue;
      const look = M.tiles?.[f];
      if (look?.wall) { tall.push({ d: x + y, wall: look.sprite || "", x, y, f }); continue; }
      if (!(look?.sprite && rpSprite(g, look.sprite, null, sx, sy, k, 0, false, 1))) {
        g.fillStyle = M.colors[f] || "#333";
        g.beginPath();
        g.moveTo(sx, sy - RP_TH * k); g.lineTo(sx + RP_TW * k, sy); g.lineTo(sx, sy + RP_TH * k); g.lineTo(sx - RP_TW * k, sy);
        g.closePath(); g.fill();
      }
    }
  }
  for (const [id, e] of RP.state) {
    const [x, y, moving] = rpPos(e);
    tall.push({ d: x + y + 0.01, id, e, x, y, moving, info: r.names[id] || { kind: "monster", name: "?", layers: [] } });
  }
  tall.sort((a, b) => a.d - b.d);
  const labels = [];
  for (const t of tall) {
    const [sx, sy] = scr(t.x, t.y);
    if (t.wall !== undefined) {
      if (!(t.wall && rpSprite(g, t.wall, null, sx, sy, k, 0, false, 1))) {
        g.fillStyle = M.colors[t.f] || "#555";
        g.fillRect(sx - RP_TW * k * 0.6, sy - 40 * k, RP_TW * k * 1.2, 40 * k);
      }
      continue;
    }
    const { e, info } = t;
    const dead = info.kind !== "node" && e.hp === 0;
    const chosen = t.id === RP.hover || t.id === RP.follow;
    if (chosen) {
      g.strokeStyle = t.id === RP.follow ? "#fff" : "rgba(255,255,255,.6)";
      g.lineWidth = 2;
      g.beginPath(); g.ellipse(sx, sy, RP_TW * k * 0.7, RP_TH * k * 0.7, 0, 0, Math.PI * 2); g.stroke();
    }
    let drawn = false;
    for (const [name, tint] of info.layers || []) drawn = rpSprite(g, name, tint, sx, sy, k, e.dir || 0, t.moving, dead ? 0.35 : 1) || drawn;
    if (!drawn) {
      g.beginPath(); g.arc(sx, sy - 12 * k, Math.max(3, 10 * k), 0, Math.PI * 2);
      g.fillStyle = KIND_COLOR[info.kind] || "#fff"; g.globalAlpha = dead ? 0.35 : 1; g.fill(); g.globalAlpha = 1;
    }
    labels.push({ sx, sy, e, info, chosen });
  }
  // Names and life over everything, as in the game.
  g.textAlign = "center";
  g.font = `${Math.max(11, Math.round(12 * Math.min(k, 1.6)))}px system-ui`;
  for (const { sx, sy, e, info, chosen } of labels) {
    const top = sy - 62 * k;
    if (info.kind !== "node" && e.hp < 100 && e.hp > 0) {
      const w = 30 * Math.max(k, 0.8);
      g.fillStyle = "rgba(0,0,0,.6)"; g.fillRect(sx - w / 2, top, w, 4);
      g.fillStyle = e.hp > 50 ? "#4ade80" : e.hp > 25 ? "#fbbf24" : "#f87171";
      g.fillRect(sx - w / 2, top, (w * e.hp) / 100, 4);
    }
    if (info.kind === "player" || info.kind === "companion" || chosen) {
      g.fillStyle = "rgba(0,0,0,.7)"; g.fillText(info.name, sx + 1, top - 5);
      g.fillStyle = info.kind === "player" ? "#fff" : KIND_COLOR[info.kind] || "#fff"; g.fillText(info.name, sx, top - 6);
    }
  }
}

// While the screen is open the scene keeps drawing: smooth steps and
// strips play between recorded frames.
function rpLoop() {
  const c = $("rp-canvas");
  if (!c || S.view !== "replays" || !RP.run) { RP.raf = 0; return; }
  paintReplay(c);
  RP.raf = requestAnimationFrame(rpLoop);
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
  const counts = { player: 0, monster: 0, npc: 0, node: 0, companion: 0 };
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
  const mx = (ev.clientX - rect.left) * (canvas.width / rect.width), my = (ev.clientY - rect.top) * (canvas.height / rect.height);
  // The nearest creature whose figure covers the point.
  let best = null, bd = 40 * Math.max(v.k, 0.5);
  for (const [id, e] of RP.state) {
    const [x, y] = rpPos(e);
    const [sx, sy] = v.scr(x, y);
    const d = Math.hypot(sx - mx, sy - 20 * v.k - my);
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

  const canvas = el("canvas", { id: "rp-canvas", class: "rp-canvas", width: 1280, height: 720 });
  // Wheel zooms, drag pans (and stops following), click follows a
  // creature, double click shows the whole zone again.
  let drag = null;
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (RP.cam) RP.cam.k = Math.max(0.25, Math.min(4, RP.cam.k * (e.deltaY < 0 ? 1.15 : 1 / 1.15)));
  }, { passive: false });
  canvas.addEventListener("mousedown", (e) => { drag = { x: e.clientX, y: e.clientY, moved: false }; });
  canvas.addEventListener("mouseup", () => { setTimeout(() => { drag = null; }, 0); });
  canvas.addEventListener("mousemove", (e) => {
    if (drag && e.buttons && RP.cam) {
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (drag.moved || Math.abs(dx) + Math.abs(dy) > 3) {
        drag.moved = true; RP.follow = null;
        const sc = canvas.width / canvas.getBoundingClientRect().width;
        RP.cam.cx -= (dx * sc) / RP.cam.k; RP.cam.cy -= (dy * sc) / RP.cam.k;
        drag.x = e.clientX; drag.y = e.clientY;
      }
      return;
    }
    const id = pickAt(canvas, e); if (id !== RP.hover) { RP.hover = id; hoverInfo(); }
  });
  canvas.addEventListener("mouseleave", () => { RP.hover = null; drag = null; hoverInfo(); });
  canvas.addEventListener("click", (e) => { if (drag?.moved) return; RP.follow = pickAt(canvas, e); hoverInfo(); });
  canvas.addEventListener("dblclick", () => { RP.follow = null; RP.cam = null; });
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
    el("p", { class: "hint rp-help" }, t("editor.replays.controls")),
    el("div", { class: "rp-cmds" }, el("span", { class: "hint" }, t("editor.replays.commands"), " "), el("span", { id: "rp-cmds" }))));

  const totals = Object.entries(r.commands).sort((a, b) => b[1] - a[1]);
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.replays.all_commands")), el("span", { class: "card-hint" }, fmt("editor.replays.seed", { n: r.seed }))),
    bars(totals, "var(--accent)")));
  requestAnimationFrame(refreshReplay);
  if (!RP.raf) RP.raf = requestAnimationFrame(rpLoop);
}
