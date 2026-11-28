// Floors and scenery: assets/tiles.ron (how each floor number of the maps
// is drawn: sprite, standing, light, day sprite) and the layout of each
// sprite in assets/sprites/sprites.ron (anchor, animation strip, light).
// Same formats the client reads; the client reloads both on its own.

const L = { tiles: null, manifest: null, saved: "", sel: null };

async function loadLooks() {
  const [tiles, manifest] = await Promise.all([api("/api/tiles"), api("/api/sprite-manifest")]);
  L.tiles = tiles;
  L.manifest = manifest;
  L.saved = JSON.stringify(tiles);
  if (!S.art) await loadArt();
  if (!S.links) await linkIndex();
  const floors = Object.keys(L.tiles).map(Number);
  if (L.sel === null || !floors.includes(L.sel)) L.sel = floors[0] ?? null;
}

function looksDirty() { return !!L.tiles && JSON.stringify(L.tiles) !== L.saved; }

// ---------- sprite layout ----------

const PLAY = ["Always", "Moving"];
const DIRS = ["S", "W", "N", "E"];

function imageOf(name) { return S.art?.kinds.sprite.images.find((i) => i.name === name); }

// What the client draws for a sprite: its own fields over its folder's.
function layoutOf(name) {
  const img = imageOf(name);
  const folder = (img && L.manifest?.folders[img.folder]) || {};
  const own = L.manifest?.sprites[name] || {};
  const pick = (k, d) => own[k] ?? folder[k] ?? d;
  let frames = pick("frames", 1), dirs = pick("directions", 1);
  // A strip that does not divide is drawn whole, as in the client.
  if (img && (![1, 4].includes(dirs) || img.width % frames || img.height % dirs)) { frames = 1; dirs = 1; }
  return { anchor: pick("anchor", [0, 0]), frames, fps: pick("fps", 8), directions: dirs, play: pick("play", "Always"),
    w: img ? img.width / frames : 0, h: img ? img.height / dirs : 0 };
}

// The first frame as a picture, for lists and pickers (the strip split
// comes with the image list, so this works before the manifest loads).
function frameThumb(name, px) {
  const img = imageOf(name);
  if (!img) return el("div", { class: "thumb letter", style: `--s:${px}px` }, icon("image", 16));
  const [fr, dr] = S.art.kinds.sprite.strips?.[name] || [1, 1];
  const l = { w: img.width / fr, h: img.height / dr };
  const k = px / Math.max(l.w, l.h);
  // A strip plays its first row on hover (CSS steps over the frames).
  return el("div", { class: "fthumb" + (fr > 1 ? " anim" : ""), style: `width:${px}px;height:${px}px` },
    el("div", { style: `width:${l.w * k}px;height:${l.h * k}px;background-image:url(${artUrl("sprite", name)});`
      + `background-size:${img.width * k}px ${img.height * k}px;--n:${fr};--fw:${l.w * k}px` }));
}

// The frame size most images of a folder have (a still image is one frame).
function frameSizeOf(folder) {
  const count = new Map();
  for (const i of S.art.kinds.sprite.images.filter((i) => i.folder === folder)) {
    const [fr, dr] = S.art.kinds.sprite.strips?.[i.name] || [1, 1];
    const k = `${i.width / fr}x${i.height / dr}`;
    count.set(k, (count.get(k) || 0) + 1);
  }
  const best = [...count.entries()].sort((a, b) => b[1] - a[1])[0];
  return best ? best[0].split("x").map(Number) : null;
}

// What an image of this size probably is in this folder: how many frames
// and rows of the folder's frame size fit.
function guessFormat(folder, w, h) {
  const fs = frameSizeOf(folder);
  if (fs && w % fs[0] === 0 && h % fs[1] === 0) {
    const frames = w / fs[0], dirs = h / fs[1];
    if ([1, 4].includes(dirs)) return { frames, directions: dirs, frame: fs };
  }
  return { frames: 1, directions: 1, frame: fs };
}

// The image cut into its frames, with the row of each direction named:
// how the strip must be drawn, shown on the image itself.
function stripDiagram(src, w, h, frames, dirs, max = 360) {
  const k = Math.max(1, Math.floor(Math.min(max / w, 220 / h)));
  const cv = el("canvas", { width: w * k + 22, height: h * k, class: "sdiag" });
  const g = cv.getContext("2d");
  g.imageSmoothingEnabled = false;
  const im = new Image();
  im.onload = () => {
    g.clearRect(0, 0, cv.width, cv.height);
    g.fillStyle = "rgba(255,255,255,.04)";
    g.fillRect(22, 0, w * k, h * k);
    g.drawImage(im, 22, 0, w * k, h * k);
    const ok = w % frames === 0 && h % dirs === 0;
    g.strokeStyle = ok ? "rgba(63,184,255,.8)" : "rgba(255,90,90,.9)";
    g.lineWidth = 1;
    for (let i = 0; i <= frames; i++) { const x = 22 + Math.round((i * w * k) / frames) + 0.5; g.beginPath(); g.moveTo(x, 0); g.lineTo(x, h * k); g.stroke(); }
    for (let j = 0; j <= dirs; j++) { const y = Math.round((j * h * k) / dirs) + 0.5; g.beginPath(); g.moveTo(22, y); g.lineTo(22 + w * k, y); g.stroke(); }
    g.fillStyle = "#9aa4b2";
    g.font = "11px sans-serif";
    if (dirs === 4) DIRS.forEach((d, j) => g.fillText(d, 6, ((j + 0.5) * h * k) / dirs + 4));
  };
  im.src = src;
  return cv;
}

// Where the feet are: the middle of the lowest solid rows of the first
// frame (the ground shadow is half see-through and left out); a floor tile
// is anchored at its middle.
async function autoAnchor(name, l, folder) {
  const im = new Image();
  im.src = artUrl("sprite", name) + "?t=" + Date.now();
  await im.decode();
  const cv = el("canvas", { width: l.w, height: l.h });
  const g = cv.getContext("2d");
  g.drawImage(im, 0, 0, l.w, l.h, 0, 0, l.w, l.h);
  const d = g.getImageData(0, 0, l.w, l.h).data;
  const solid = (x, y) => d[(y * l.w + x) * 4 + 3] === 255;
  let x0 = l.w, x1 = -1, y0 = l.h, y1 = -1;
  for (let y = 0; y < l.h; y++) for (let x = 0; x < l.w; x++) if (solid(x, y)) {
    x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
  }
  if (x1 < 0) return null;
  if (folder === "tiles") return [Math.round((x0 + x1 + 1) / 2), Math.round((y0 + y1 + 1) / 2)];
  let sum = 0, n = 0;
  for (let y = Math.max(y0, y1 - 3); y <= y1; y++) for (let x = 0; x < l.w; x++) if (solid(x, y)) { sum += x; n++; }
  return [Math.round(sum / n), y1];
}

// A sprite standing on its tile, playing its strip: what the game shows.
// `o.dir` (0..3: S W N E), `o.moving`, `o.light` ({radius, color, strength}),
// `o.cross` draws the anchor; `o.onpick(x, y)` gets a clicked image pixel.
function spritePreview(name, o = {}) {
  const W = 240, H = 220, cx = W / 2, cy = 150;
  let SC = Number(storeGet("editor-ui.preview_zoom")) || 2;
  const cv = el("canvas", { width: W, height: H, class: "spreview" });
  const g = cv.getContext("2d");
  g.imageSmoothingEnabled = false;
  const im = new Image();
  im.src = artUrl("sprite", name);
  const t0 = performance.now();
  const frame = () => {
    if (!cv.isConnected && performance.now() - t0 > 2000) return;
    const l = o.layout ? o.layout() : layoutOf(name);
    g.clearRect(0, 0, W, H);
    // The tile: 64x32 diamond.
    g.fillStyle = "rgba(120,140,110,.35)";
    g.strokeStyle = "rgba(255,255,255,.25)";
    g.beginPath();
    g.moveTo(cx, cy - 16 * SC); g.lineTo(cx + 32 * SC, cy); g.lineTo(cx, cy + 16 * SC); g.lineTo(cx - 32 * SC, cy);
    g.closePath(); g.fill(); g.stroke();
    const light = o.light ? o.light() : null;
    if (light) {
      const [r, gg, b] = light.color.map((c) => Math.round(c * 255));
      const rad = Math.min(light.radius * 18 * SC, W);
      const grad = g.createRadialGradient(cx, cy, 0, cx, cy, rad);
      grad.addColorStop(0, `rgba(${r},${gg},${b},${0.55 * (light.strength ?? 1)})`);
      grad.addColorStop(1, `rgba(${r},${gg},${b},0)`);
      g.fillStyle = grad;
      g.fillRect(0, 0, W, H);
    }
    if (im.complete && im.naturalWidth && l.w) {
      const playing = l.frames > 1 && (l.play === "Always" || (o.moving && o.moving()));
      const f = playing ? Math.floor(((performance.now() - t0) / 1000) * l.fps) % l.frames : 0;
      const row = l.directions === 4 ? (o.dir ? o.dir() : 0) : 0;
      g.drawImage(im, f * l.w, row * l.h, l.w, l.h, cx - l.anchor[0] * SC, cy - l.anchor[1] * SC, l.w * SC, l.h * SC);
      if (o.cross) {
        g.strokeStyle = "#ff4d6d";
        g.beginPath();
        g.moveTo(cx - 8, cy); g.lineTo(cx + 8, cy); g.moveTo(cx, cy - 8); g.lineTo(cx, cy + 8);
        g.stroke();
      }
    }
    requestAnimationFrame(frame);
  };
  im.onload = () => requestAnimationFrame(frame);
  if (o.onpick) {
    cv.style.cursor = "crosshair";
    cv.addEventListener("click", (e) => {
      const r = cv.getBoundingClientRect();
      const l = o.layout ? o.layout() : layoutOf(name);
      const x = ((e.clientX - r.left) * (W / r.width) - (cx - l.anchor[0] * SC)) / SC;
      const y = ((e.clientY - r.top) * (H / r.height) - (cy - l.anchor[1] * SC)) / SC;
      o.onpick(Math.round(x), Math.round(y));
    });
  }
  // Zoom, remembered for the next preview.
  const zoom = el("div", { class: "seg zoomseg" }, [1, 2, 3].map((z) => el("button", { class: "ghost sm" + (z === SC ? " on" : ""),
    onclick: (e) => { SC = z; storeSet("editor-ui.preview_zoom", z); zoom.querySelectorAll("button").forEach((b) => b.classList.remove("on")); e.currentTarget.classList.add("on"); } }, `${z}×`)));
  return el("div", { class: "pvbox" }, cv, zoom);
}

function numIn(value, placeholder, attrs = {}) {
  return el("input", { type: "number", class: "num", value: value ?? "", placeholder: placeholder ?? "", ...attrs });
}

const toHex = (c) => "#" + c.map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, "0")).join("");
const fromHex = (h) => [1, 3, 5].map((i) => Math.round((parseInt(h.slice(i, i + 2), 16) / 255) * 100) / 100);

// Light fields: on/off, radius, color, strength. `get`/`set` hold
// `{radius, color, strength}` or null.
function lightFields(get, set, redrawIt) {
  const box = el("div", { class: "lightf" });
  const fill = () => {
    const l = get();
    const on = el("input", { type: "checkbox", checked: !!l,
      onchange: (e) => { set(e.target.checked ? { radius: 4, color: [1, 0.62, 0.28], strength: 1 } : null); fill(); redrawIt(); } });
    box.replaceChildren(el("label", { class: "check" }, on, t("editor.looks.shines")));
    if (!l) return;
    box.append(el("div", { class: "row3" },
      el("div", { class: "fld" }, el("label", {}, t("editor.looks.radius")),
        numIn(l.radius, "", { min: 0.5, max: 32, step: 0.5, oninput: (e) => { l.radius = Number(e.target.value) || 0; redrawIt(); } })),
      el("div", { class: "fld" }, el("label", {}, t("editor.looks.color")),
        el("input", { type: "color", value: toHex(l.color), oninput: (e) => { l.color = fromHex(e.target.value); redrawIt(); } })),
      el("div", { class: "fld" }, el("label", {}, t("editor.looks.strength")),
        el("input", { type: "range", min: 0.1, max: 1, step: 0.05, value: l.strength ?? 1,
          oninput: (e) => { l.strength = Number(e.target.value); redrawIt(); } }))));
  };
  fill();
  return box;
}

// Anchor, strip and light of one sprite (or the defaults of a folder).
async function layoutDialog(target, folder) {
  if (!L.manifest) await loadLooks();
  const isFolder = !!folder;
  const map = isFolder ? L.manifest.folders : L.manifest.sprites;
  const draft = { ...(map[target] || {}) };
  let light = isFolder ? null : (L.manifest.lights[target] ? { ...L.manifest.lights[target] } : null);
  const sample = isFolder ? S.art.kinds.sprite.images.find((i) => i.folder === target)?.name : target;
  const base = isFolder ? {} : (L.manifest.folders[imageOf(target)?.folder] || {});
  let dir = 0, moving = true;
  const current = () => {
    const img = imageOf(sample);
    const pick = (k, d) => draft[k] ?? (isFolder ? undefined : base[k]) ?? d;
    let frames = pick("frames", 1), dirs = pick("directions", 1);
    if (img && (![1, 4].includes(dirs) || img.width % frames || img.height % dirs)) { frames = 1; dirs = 1; }
    return { anchor: pick("anchor", [0, 0]), frames, fps: pick("fps", 8), directions: dirs, play: pick("play", "Always"),
      w: img ? img.width / frames : 0, h: img ? img.height / dirs : 0 };
  };
  const setNum = (k, v) => { if (v === "" || v === null || Number.isNaN(Number(v))) delete draft[k]; else draft[k] = Number(v); };
  const ax = numIn(draft.anchor?.[0], base.anchor?.[0], { oninput: () => syncAnchor() });
  const ay = numIn(draft.anchor?.[1], base.anchor?.[1], { oninput: () => syncAnchor() });
  const syncAnchor = () => {
    if (ax.value === "" && ay.value === "") delete draft.anchor;
    else draft.anchor = [Number(ax.value || 0), Number(ay.value || 0)];
    warn();
  };
  const frames = numIn(draft.frames, base.frames ?? 1, { min: 1, max: 64, oninput: (e) => { setNum("frames", e.target.value); warn(); } });
  const fps = numIn(draft.fps, base.fps ?? 8, { min: 1, max: 60, step: 0.5, oninput: (e) => setNum("fps", e.target.value) });
  const dirs = el("select", { onchange: (e) => { if (e.target.value) draft.directions = Number(e.target.value); else delete draft.directions; warn(); } },
    el("option", { value: "" }, `— ${base.directions ?? 1}`),
    [1, 4].map((d) => el("option", { value: d, selected: draft.directions === d }, t(d === 4 ? "editor.looks.dirs4" : "editor.looks.dirs1"))));
  const play = el("select", { onchange: (e) => { if (e.target.value) draft.play = e.target.value; else delete draft.play; } },
    el("option", { value: "" }, `— ${t("editor.looks.play_" + (base.play ?? "Always").toLowerCase())}`),
    PLAY.map((p) => el("option", { value: p, selected: draft.play === p }, t("editor.looks.play_" + p.toLowerCase()))));
  const note = el("p", { class: "hint warnline" });
  const diag = el("div");
  const quick = el("div", { class: "btnrow" });
  const setFormat = (f, d) => {
    const bf = base.frames ?? 1, bd = base.directions ?? 1;
    if (f === bf && !isFolder) delete draft.frames; else draft.frames = f;
    if (d === bd && !isFolder) delete draft.directions; else draft.directions = d;
    frames.value = draft.frames ?? ""; dirs.value = draft.directions ?? "";
    warn();
  };
  const warn = () => {
    const img = imageOf(sample);
    const f = draft.frames ?? base.frames ?? 1, d = draft.directions ?? base.directions ?? 1;
    const split = img && (img.width % f || img.height % d);
    // A frame unlike the folder's (a still image cut by the folder's 4 x 4).
    const usual = img && frameSizeOf(img.folder);
    const odd = img && !split && !isFolder && usual && (img.width / f !== usual[0] || img.height / d !== usual[1]);
    const bad = split || odd;
    note.textContent = split ? t("editor.looks.bad_strip").replace("{w}", img.width).replace("{h}", img.height)
      : odd ? t("editor.looks.frame_is").replace("{w}", img.width / f).replace("{h}", img.height / d) + " "
        + t("editor.looks.frame_differs").replace("{w}", usual[0]).replace("{h}", usual[1]) : "";
    if (img) diag.replaceChildren(stripDiagram(artUrl("sprite", sample), img.width, img.height, f, d, 300));
    quick.replaceChildren();
    if (img && bad) {
      const g = guessFormat(img.folder, img.width, img.height);
      quick.append(el("button", { class: "ghost sm", onclick: () => setFormat(1, 1) }, t("editor.looks.make_still")));
      if (g.frames * g.directions > 1) quick.append(el("button", { class: "ghost sm", onclick: () => setFormat(g.frames, g.directions) },
        t("editor.looks.use_guess").replace("{f}", g.frames).replace("{d}", g.directions)));
    }
  };
  const dirBtns = el("div", { class: "seg" }, DIRS.map((d, i) => el("button", { class: "ghost sm" + (i === 0 ? " on" : ""),
    onclick: (e) => { dir = i; dirBtns.querySelectorAll("button").forEach((b) => b.classList.remove("on")); e.currentTarget.classList.add("on"); } }, d)));
  const walk = el("label", { class: "check" }, el("input", { type: "checkbox", checked: moving, onchange: (e) => { moving = e.target.checked; } }),
    t("editor.looks.walking"));
  const imgOf = imageOf(sample);
  const autoBtn = sample ? el("button", { class: "ghost sm", onclick: async () => {
    const a = await autoAnchor(sample, current(), imgOf?.folder);
    if (a) { ax.value = a[0]; ay.value = a[1]; syncAnchor(); }
  } }, t("editor.looks.auto_anchor_btn")) : null;
  // Copy anchor, strip and light from a sprite already right.
  const copy = isFolder ? null : el("select", { onchange: (e) => {
    const from = e.target.value;
    if (!from) return;
    const own = L.manifest.sprites[from] || {};
    for (const k of ["anchor", "frames", "fps", "directions", "play"]) { if (own[k] !== undefined) draft[k] = structuredClone(own[k]); else delete draft[k]; }
    ax.value = draft.anchor?.[0] ?? ""; ay.value = draft.anchor?.[1] ?? "";
    frames.value = draft.frames ?? ""; fps.value = draft.fps ?? ""; dirs.value = draft.directions ?? ""; play.value = draft.play ?? "";
    e.target.value = "";
    warn();
  } }, el("option", { value: "" }, t("editor.looks.copy_from")),
    Object.keys(L.manifest.sprites).filter((n) => n !== target).sort().map((n) => el("option", { value: n }, n)));
  warn();
  const preview = sample ? spritePreview(sample, { layout: current, dir: () => dir, moving: () => moving, light: () => light, cross: true,
    onpick: (x, y) => { const a = current().anchor; ax.value = a[0] + x; ay.value = a[1] + y; syncAnchor(); } }) : el("p", { class: "hint" }, t("editor.looks.no_sample"));
  const body = [
    el("div", { class: "layout-dlg" },
      el("div", { class: "lcol" }, preview, el("p", { class: "hint" }, t("editor.looks.anchor_hint")), dirBtns, walk, diag),
      el("div", { class: "lcol grow" },
        isFolder ? el("p", { class: "hint" }, t("editor.looks.folder_hint")) : el("p", { class: "hint" }, t("editor.looks.inherit_hint")),
        el("div", { class: "btnrow" }, autoBtn, copy),
        el("div", { class: "row2" }, el("div", { class: "fld" }, el("label", {}, t("editor.looks.anchor") + " X"), ax),
          el("div", { class: "fld" }, el("label", {}, t("editor.looks.anchor") + " Y"), ay)),
        el("div", { class: "row2" }, el("div", { class: "fld" }, el("label", {}, t("editor.looks.frames")), frames),
          el("div", { class: "fld" }, el("label", {}, t("editor.looks.fps")), fps)),
        el("div", { class: "row2" }, el("div", { class: "fld" }, el("label", {}, t("editor.looks.directions")), dirs),
          el("div", { class: "fld" }, el("label", {}, t("editor.looks.play")), play)),
        el("p", { class: "hint" }, t("editor.looks.strip_hint")), note, quick,
        isFolder ? null : lightFields(() => light, (v) => { light = v; }, () => {}))),
  ];
  const ok = await dialog(`${t("editor.looks.layout")}: ${target}`, body, [
    { label: t("editor.cancel"), value: false },
    { label: t("editor.save"), value: true, primary: true },
  ], { wide: true });
  if (!ok) return false;
  const next = structuredClone(L.manifest);
  const m = isFolder ? next.folders : next.sprites;
  if (Object.keys(draft).length) m[target] = draft; else delete m[target];
  if (!isFolder) { if (light) next.lights[target] = light; else delete next.lights[target]; }
  try {
    await api("/api/sprite-manifest", next);
    L.manifest = next;
    await loadArt();
    toast(t("editor.looks.saved_manifest"));
    return true;
  } catch (e) {
    await say(t("editor.looks.layout"), e.message);
    return false;
  }
}

// ---------- format on upload ----------

// The "what is this image" part of the upload dialog: still image or strip,
// guessed from the folder's frame size and shown cut on the image itself.
async function uploadFormat(url, folder) {
  const im = new Image();
  im.src = url;
  try { await im.decode(); } catch (_) { return null; }
  const w = im.naturalWidth, h = im.naturalHeight;
  const g = guessFormat(folder, w, h);
  const fl = (L.manifest || await api("/api/sprite-manifest")).folders[folder] || {};
  const st = { strip: g.frames * g.directions > 1, frames: g.frames, directions: g.directions,
    play: fl.play ?? (folder === "creatures" ? "Moving" : "Always"), auto: !fl.anchor };
  const diag = el("div");
  const frames = numIn(st.frames, "", { min: 1, max: 64, oninput: (e) => { st.frames = Math.max(1, Number(e.target.value) || 1); fill(); } });
  const dirs = el("select", { onchange: (e) => { st.directions = Number(e.target.value); fill(); } },
    [1, 4].map((d) => el("option", { value: d, selected: st.directions === d }, t(d === 4 ? "editor.looks.dirs4" : "editor.looks.dirs1"))));
  const play = el("select", { onchange: (e) => { st.play = e.target.value; } },
    PLAY.map((p) => el("option", { value: p, selected: st.play === p }, t("editor.looks.play_" + p.toLowerCase()))));
  const radio = (on, label) => el("label", { class: "check" }, el("input", { type: "radio", name: "fmt", checked: st.strip === on,
    onchange: () => { st.strip = on; fill(); } }), label);
  const stripFields = el("div", { class: "row3" },
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.frames")), frames),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.directions")), dirs),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.play")), play));
  const msg = el("p", { class: "hint" });
  const fill = () => {
    const f = st.strip ? st.frames : 1, d = st.strip ? st.directions : 1;
    stripFields.hidden = !st.strip;
    diag.replaceChildren(stripDiagram(url, w, h, f, d));
    const ok = w % f === 0 && h % d === 0;
    msg.className = "hint" + (ok ? "" : " warnline");
    msg.textContent = !ok ? t("editor.looks.bad_strip").replace("{w}", w).replace("{h}", h)
      : t("editor.looks.frame_is").replace("{w}", w / f).replace("{h}", h / d)
        + (g.frame && (w / f !== g.frame[0] || h / d !== g.frame[1]) ? " " + t("editor.looks.frame_differs").replace("{w}", g.frame[0]).replace("{h}", g.frame[1]) : "");
  };
  fill();
  const box = el("div", { class: "fmt" },
    el("strong", {}, t("editor.looks.what_is")),
    el("div", { class: "radios" }, radio(false, t("editor.looks.still")), radio(true, t("editor.looks.strip"))),
    g.frames * g.directions > 1 ? el("p", { class: "hint" }, t("editor.looks.guessed").replace("{f}", g.frames).replace("{d}", g.directions)) : null,
    stripFields, diag, msg,
    el("label", { class: "check" }, el("input", { type: "checkbox", checked: st.auto, onchange: (e) => { st.auto = e.target.checked; } }),
      t("editor.looks.auto_anchor")),
    el("p", { class: "hint" }, t("editor.looks.strip_hint")));
  return { box, value: () => ({ frames: st.strip ? st.frames : 1, directions: st.strip ? st.directions : 1, play: st.play, auto: st.auto }) };
}

// Writes what the upload said into sprites.ron: only what differs from the
// folder, so a sprite like its folder stays out of the file.
async function saveFormat(name, folder, v) {
  if (!L.manifest) await loadLooks();
  const fl = L.manifest.folders[folder] || {};
  const next = structuredClone(L.manifest);
  const own = { ...(next.sprites[name] || {}) };
  const put = (k, val, dflt) => { if (val === (fl[k] ?? dflt)) delete own[k]; else own[k] = val; };
  put("frames", v.frames, 1);
  put("directions", v.directions, 1);
  if (v.frames > 1) put("play", v.play, "Always"); else delete own.play;
  if (v.auto) {
    const img = imageOf(name);
    const a = img && await autoAnchor(name, { w: img.width / v.frames, h: img.height / v.directions }, folder);
    if (a && !(fl.anchor && fl.anchor[0] === a[0] && fl.anchor[1] === a[1])) own.anchor = a;
  }
  if (Object.keys(own).length) next.sprites[name] = own; else delete next.sprites[name];
  if (JSON.stringify(next) === JSON.stringify(L.manifest)) return;
  try {
    await api("/api/sprite-manifest", next);
    L.manifest = next;
    await loadArt();
    if (S.view === "art") { drawArtSide(); drawArt(); }
  } catch (e) {
    await say(t("editor.looks.layout"), e.message);
  }
}

// ---------- floors ----------

function floorUses(floor) {
  const out = [];
  for (const f of S.links?.all || []) {
    if (f.kind !== "map") continue;
    for (const [ch, d] of Object.entries(f.json?.legend || {})) {
      if (d.floor === floor) out.push({ zone: f.id, ch, sw: d.switch ?? null, path: f.path });
      else if (d.switch === floor) out.push({ zone: f.id, ch, from: d.floor, path: f.path });
    }
  }
  return out;
}

function drawLooksSide() {
  const side = $("side");
  side.replaceChildren();
  side.append(el("div", { class: "side-head" }, el("h2", {}, t("editor.looks")), el("div", { class: "grow" }),
    el("button", { class: "icon-btn", title: t("editor.looks.new_floor"), onclick: newFloor }, icon("plus"))));
  side.append(searchBox(S.filter, (v) => { S.filter = v; drawLooksSide(); }));
  const q = norm(S.filter);
  const list = el("div", { class: "list" });
  for (const [k, look] of Object.entries(L.tiles)) {
    const n = Number(k);
    if (q && !norm(`${n} ${look.sprite} ${look.day_sprite || ""} ${look.rain_sprite || ""} ${look.snow_sprite || ""}`).includes(q)) continue;
    const tags = [look.wall ? t("editor.looks.standing") : null, look.light ? t("editor.looks.shines") : null,
      floorUses(n).some((u) => u.sw || u.from) ? t("editor.looks.switches") : null].filter(Boolean).join(" · ");
    list.append(el("button", { class: "entry" + (L.sel === n ? " sel" : ""), onclick: () => { L.sel = n; drawLooksSide(); drawLooks(); } },
      frameThumb(look.sprite, 36),
      el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, `${n} · ${look.sprite || t("editor.no_image")}`),
        el("div", { class: "entry-sub" }, tags))));
  }
  side.append(list);
}

function mapFiles() { return (S.links?.all || []).filter((f) => f.kind === "map"); }

// Writes changed map files the way the content screens do (checked first).
async function saveMaps(files) {
  if (!files.length) return true;
  const open = files.find((f) => f.path === S.sel);
  if (open && S.original !== undefined && typeof snap === "function" && S.original !== snap()) {
    await say(t("editor.looks"), t("editor.looks.map_dirty").replace("{zone}", open.id));
    return false;
  }
  const r = await api("/api/save", { files: files.map((f) => ({ path: f.path, json: clean(f.json) })), texts: [] });
  if (!r.ok) { await say(t("editor.looks"), r.report.errors.join(" · ")); return false; }
  await reloadContent();
  S.links = null;
  await linkIndex();
  if (open) { S.sel = null; }
  return true;
}

// A new floor in one go: its picture, standing or not, its light, and the
// map legend it goes into (so it can be painted right away).
async function newFloor() {
  if (looksDirty() && !(await askLeave())) return;
  const used = Object.keys(L.tiles).map(Number);
  const n = Math.max(0, ...used) + 1;
  const look = { sprite: "", wall: false, light: null, day_sprite: null, rain_sprite: null, snow_sprite: null };
  const wall = el("input", { type: "checkbox" });
  const pick = artPicker("sprite", "", (v) => {
    look.sprite = v || "";
    // Something drawn in the walls folder stands up.
    if (imageOf(look.sprite)?.folder === "walls") wall.checked = true;
  });
  const zones = mapFiles();
  const zone = el("select", {}, el("option", { value: "" }, t("editor.looks.no_map")),
    zones.map((f) => el("option", { value: f.path }, f.id)));
  const ok = await dialog(`${t("editor.looks.new_floor")} ${n}`, [
    el("p", { class: "hint" }, t("editor.looks.new_floor_hint")),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.sprite")), pick),
    el("label", { class: "check" }, wall, t("editor.looks.standing")),
    el("p", { class: "hint" }, t("editor.looks.standing_hint")),
    lightFields(() => look.light, (v) => { look.light = v; }, () => {}),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.add_to_map")), zone,
      el("span", { class: "hint" }, t("editor.looks.add_to_map_hint"))),
  ], [
    { label: t("editor.cancel"), value: false },
    { label: t("editor.add"), value: true, primary: true },
  ], { wide: true, validate: () => !!look.sprite || (toast(t("editor.looks.need_sprite"), "err"), false) });
  if (!ok) return;
  look.wall = wall.checked;
  L.tiles[n] = look;
  L.sel = n;
  await saveTiles();
  const f = zones.find((z) => z.path === zone.value);
  if (f) {
    const legend = f.json.legend ||= {};
    const ch = charFor(look.sprite, new Set(Object.keys(legend)));
    legend[ch] = look.wall ? { floor: n, blocks_movement: true, blocks_sight: true } : { floor: n };
    if (await saveMaps([f])) toast(t("editor.looks.added_to_map").replace("{zone}", f.id).replace("{ch}", ch));
  }
  drawLooksSide();
  drawLooks();
}

// Every legend tile of every map painted with `a` switches to `b`, and
// every one painted with `b` back to `a` (b = null: no switch).
function pairChanges(a, b) {
  const out = [];
  for (const f of mapFiles()) {
    const json = structuredClone(f.json);
    let n = 0;
    for (const d of Object.values(json.legend || {})) {
      let want;
      if (d.floor === a) want = b;
      else if (b !== null && d.floor === b) want = a;
      else if (d.switch === a) want = null; // the old partner lets go
      else continue;
      if ((d.switch ?? null) === want) continue;
      if (want === null) delete d.switch; else d.switch = want;
      n++;
    }
    if (n) out.push({ ...f, json, n });
  }
  return out;
}

async function saveTiles() {
  try {
    await api("/api/tiles", L.tiles);
    L.saved = JSON.stringify(L.tiles);
    M.tiles = null; // the map painter reads them again
    toast(t("editor.looks.saved"));
    drawLooks();
  } catch (e) {
    await say(t("editor.looks"), e.message);
  }
}

function drawLooks() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page" });
  const look = L.tiles[L.sel];
  const save = el("button", { class: "primary", disabled: !looksDirty(), onclick: saveTiles }, icon("check", 16), t("editor.save"));
  const undo = el("button", { class: "ghost", disabled: !looksDirty(),
    onclick: () => { L.tiles = JSON.parse(L.saved); if (!L.tiles[L.sel]) L.sel = Number(Object.keys(L.tiles)[0]); drawLooksSide(); drawLooks(); } },
    t("editor.looks.discard"));
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, t("editor.looks")),
      el("h1", {}, look ? `${t("editor.looks.floor")} ${L.sel}` : t("editor.looks"))),
    el("div", { class: "grow" }), undo, save));
  page.append(el("p", { class: "hint" }, t("editor.looks.intro")));
  if (!look) { p.append(page); return; }
  const changed = () => { save.disabled = undo.disabled = !looksDirty(); drawLooksSide(); };
  const again = () => { changed(); drawLooks(); };
  // Look.
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.looks.look"))),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.sprite")),
      artPicker("sprite", look.sprite, (v) => { look.sprite = v || ""; again(); })),
    look.sprite && imageOf(look.sprite) ? el("button", { class: "ghost sm", onclick: async () => { if (await layoutDialog(look.sprite)) drawLooks(); } },
      t("editor.looks.edit_layout")) : null,
    el("label", { class: "check" }, el("input", { type: "checkbox", checked: !!look.wall,
      onchange: (e) => { look.wall = e.target.checked; changed(); } }), t("editor.looks.standing")),
    el("p", { class: "hint" }, t("editor.looks.standing_hint"))));
  // Light.
  const pv = el("div", { class: "pvrow" });
  const fillPv = () => pv.replaceChildren(...[
    look.sprite ? el("figure", {}, spritePreview(look.sprite, { light: () => look.light }), el("figcaption", {}, t("editor.looks.night"))) : null,
    look.day_sprite ? el("figure", {}, spritePreview(look.day_sprite), el("figcaption", {}, t("editor.looks.day"))) : null,
    look.rain_sprite ? el("figure", {}, spritePreview(look.rain_sprite), el("figcaption", {}, t("editor.looks.rain"))) : null,
    look.snow_sprite ? el("figure", {}, spritePreview(look.snow_sprite), el("figcaption", {}, t("editor.looks.snow"))) : null,
  ].filter(Boolean));
  fillPv();
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.looks.light"))),
    lightFields(() => look.light, (v) => { look.light = v; }, changed),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.day_sprite")),
      artPicker("sprite", look.day_sprite || "", (v) => { look.day_sprite = v || null; again(); }),
      el("span", { class: "hint" }, t("editor.looks.day_sprite_hint")))));
  // Weather: other sprites while it rains or snows (picture only).
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.looks.weather"))),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.rain_sprite")),
      artPicker("sprite", look.rain_sprite || "", (v) => { look.rain_sprite = v || null; again(); })),
    el("div", { class: "fld" }, el("label", {}, t("editor.looks.snow_sprite")),
      artPicker("sprite", look.snow_sprite || "", (v) => { look.snow_sprite = v || null; again(); }),
      el("span", { class: "hint" }, t("editor.looks.weather_hint")))));
  page.append(el("section", { class: "card" }, el("div", { class: "card-head" }, el("h3", {}, t("editor.looks.preview"))), pv));
  // Switch pair (a brazier lit and put out): set once here for every map.
  const current = floorUses(L.sel).find((u) => u.sw)?.sw ?? null;
  const other = el("select", {}, el("option", { value: "" }, t("editor.map.switch_none")),
    Object.entries(L.tiles).filter(([k]) => Number(k) !== L.sel)
      .map(([k, l]) => el("option", { value: k, selected: Number(k) === current }, `${k} · ${l.sprite}`)));
  const pairInfo = el("span", { class: "hint" });
  const pairPv = el("div", { class: "pvrow" });
  const showPair = () => {
    const b = other.value ? Number(other.value) : null;
    const ch = pairChanges(L.sel, b);
    pairInfo.textContent = ch.length ? t("editor.looks.pair_changes").replace("{n}", ch.reduce((s, c) => s + c.n, 0)).replace("{maps}", ch.map((c) => c.id).join(", "))
      : t("editor.looks.pair_same");
    apply.disabled = !ch.length;
    pairPv.replaceChildren(...[look.sprite ? frameThumb(look.sprite, 56) : null, b && L.tiles[b] ? el("span", { class: "pair-arrow" }, "⇄") : null,
      b && L.tiles[b]?.sprite ? frameThumb(L.tiles[b].sprite, 56) : null].filter(Boolean));
  };
  const apply = el("button", { class: "primary sm", onclick: async () => {
    if (looksDirty()) { await say(t("editor.looks"), t("editor.looks.save_first")); return; }
    if (await saveMaps(pairChanges(L.sel, other.value ? Number(other.value) : null))) { toast(t("editor.looks.pair_saved")); drawLooksSide(); drawLooks(); }
  } }, t("editor.looks.pair_apply"));
  other.addEventListener("change", showPair);
  showPair();
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.looks.pair"))),
    el("p", { class: "hint" }, t("editor.looks.pair_hint")),
    el("div", { class: "pairrow" }, pairPv, el("div", { class: "fld" }, el("label", {}, t("editor.map.switch")), other)),
    el("div", { class: "btnrow" }, apply, pairInfo)));
  // Where it is painted.
  const uses = floorUses(L.sel);
  page.append(el("section", { class: "card" },
    el("div", { class: "card-head" }, el("h3", {}, t("editor.looks.in_maps"))),
    uses.length ? el("div", { class: "uses" }, uses.map((u) => el("div", { class: "use-row" },
      el("strong", {}, u.zone), el("code", {}, `'${u.ch}'`),
      el("span", { class: "hint" }, u.sw ? t("editor.looks.switch_to").replace("{n}", u.sw)
        : u.from ? t("editor.looks.switch_from").replace("{n}", u.from) : ""),
      el("button", { class: "ghost sm", onclick: () => openFile(u.path) }, t("editor.links.open_map")))))
      : el("p", { class: "hint" }, t("editor.looks.unused")),
    el("p", { class: "hint" }, t("editor.looks.switch_hint"))));
  p.append(page);
}

// Opens a content file as the rest of the editor does.
async function openFile(path) {
  if (looksDirty() && !(await askLeave())) return;
  L.saved = JSON.stringify(L.tiles);
  S.view = "content";
  await open(path);
}
