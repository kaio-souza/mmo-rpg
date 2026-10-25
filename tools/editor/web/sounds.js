// Sounds: which sound plays for each event the game
// fires, for each technique, the music of each zone and of the screens
// outside the world (P05)
// (assets/sounds/sounds.ron), plus the list of sound files with license and
// credit. Files arrive already in OGG Vorbis; the editor never
// encodes. The client reads the file again a second after a save.

const SND = { info: null, cfg: null, saved: "", player: null };

const MAX_VARIANTS = 8;

async function loadSounds() {
  SND.info = await api("/api/sounds");
  SND.cfg = SND.info.config;
  SND.cfg.sounds ??= {};
  SND.cfg.events ??= {};
  SND.cfg.techniques ??= {};
  SND.cfg.zones ??= {};
  SND.cfg.screens ??= {};
  SND.saved = JSON.stringify(SND.cfg);
}

function soundsDirty() { return !!SND.cfg && JSON.stringify(SND.cfg) !== SND.saved; }

function evLabel(id) { return t("editor.sounds.event." + id); }

// Plays a file in the page; a second click cuts the first.
function playSound(file, volume = 1) {
  if (!file) return;
  if (SND.player) SND.player.pause();
  SND.player = new Audio("/api/sound-file?path=" + encodeURIComponent(file));
  SND.player.volume = Math.max(0, Math.min(1, volume));
  SND.player.play().catch((e) => toast(e.message, "err"));
}

function stopSound() { if (SND.player) SND.player.pause(); }

function soundFile(name) { return SND.cfg.sounds[name]?.file; }

function soundNames() { return Object.keys(SND.cfg.sounds).sort(); }

function soundUsers(name) {
  const ev = Object.entries(SND.cfg.events).filter(([, e]) => e.sounds.includes(name)).map(([id]) => evLabel(id));
  const tech = Object.entries(SND.cfg.techniques).filter(([, e]) => e.sounds.includes(name)).map(([id]) => id);
  const zones = Object.entries(SND.cfg.zones).filter(([, z]) => z.music === name).map(([id]) => id);
  const screens = Object.entries(SND.cfg.screens).filter(([, z]) => z.music === name).map(([id]) => t("editor.sounds.screen." + id));
  return [...ev, ...tech, ...zones, ...screens];
}

function jump(id) { document.getElementById(id)?.scrollIntoView({ behavior: "smooth" }); }

function drawSoundsSide() {
  const side = $("side");
  side.replaceChildren(el("div", { class: "side-head" }, el("h2", {}, t("editor.sounds"))));
  const list = el("div", { class: "list" });
  const entry = (anchor, ic, title, sub, n) => list.append(el("div", { class: "entry", onclick: () => jump(anchor) },
    el("div", { class: "thumb letter", style: `--s:36px;--h:${hue(anchor)}` }, icon(ic, 18)),
    el("div", { class: "entry-txt" }, el("div", { class: "entry-title" }, title), el("div", { class: "hint mono" }, sub)),
    el("span", { class: "chip" + (n ? "" : " warn") }, n)));
  entry("snd-events", "sound", t("editor.sounds.events"), "events", Object.values(SND.cfg.events).filter((e) => e.sounds.length).length);
  entry("snd-techniques", "sound", t("editor.sounds.techniques"), "techniques", Object.keys(SND.cfg.techniques).length);
  entry("snd-zones", "sound", t("editor.sounds.zones"), "zones", Object.keys(SND.cfg.zones).length);
  entry("snd-screens", "sound", t("editor.sounds.screens"), "screens", Object.keys(SND.cfg.screens).length);
  entry("snd-library", "sound", t("editor.sounds.library"), "sounds", soundNames().length);
  side.append(list);
}

async function saveSounds() {
  try {
    await api("/api/sounds", SND.cfg);
  } catch (e) {
    await say(t("editor.sounds"), e.message);
    return;
  }
  await loadSounds();
  toast(t("editor.sounds.saved"));
  draw();
}

// Variations, volume, priority and "comes from where it happens" of one
// event or technique. `map` is SND.cfg.events or SND.cfg.techniques.
function bindingCard(map, id, title, hint, interfaceOnly, onRemove) {
  const e = map[id];
  const names = soundNames();
  const ensure = () => (map[id] ??= { sounds: [], volume: 1000, priority: 0, positional: !interfaceOnly });
  const card = el("section", { class: "card", id: "bind-" + id });
  card.append(el("div", { class: "lang-head" },
    el("div", { class: "grow" }, el("strong", {}, title), " ", el("code", {}, id), el("div", { class: "hint" }, hint)),
    onRemove ? el("button", { class: "ghost sm danger", title: t("editor.sounds.remove"), onclick: onRemove }, "✕") : null));
  if (!e || !e.sounds.length) card.append(el("p", { class: "hint warnline" }, t("editor.sounds.silent")));
  (e?.sounds || []).forEach((name, i) => {
    card.append(el("div", { class: "btnrow" },
      el("select", { onchange: (x) => { e.sounds[i] = x.target.value; draw(); } },
        names.map((n) => el("option", { value: n, selected: n === name }, n))),
      el("button", { class: "ghost sm", title: t("editor.sounds.play"), onclick: () => playSound(soundFile(name), e.volume / 1000) }, "▶"),
      el("button", { class: "ghost sm danger", title: t("editor.sounds.remove"), onclick: () => { e.sounds.splice(i, 1); draw(); } }, "✕")));
  });
  const full = (e?.sounds.length || 0) >= MAX_VARIANTS;
  card.append(el("div", { class: "btnrow" },
    el("button", { class: "ghost sm", disabled: !names.length || full,
      onclick: () => { ensure().sounds.push(names[0]); draw(); } }, icon("plus", 14), t("editor.sounds.add_variant")),
    el("button", { class: "ghost sm", disabled: full,
      onclick: () => uploadSound((name) => ensure().sounds.push(name)) }, "⇪ ", t("editor.sounds.upload_use"))));
  if (e) {
    const pct = el("span", { class: "hint mono" }, Math.round(e.volume / 10) + "%");
    card.append(
      el("div", { class: "fld" }, el("label", {}, t("editor.sounds.volume")),
        el("div", { class: "btnrow" },
          el("input", { type: "range", min: 0, max: 1000, step: 10, value: e.volume,
            oninput: (x) => { e.volume = Number(x.target.value); pct.textContent = Math.round(e.volume / 10) + "%"; },
            onchange: () => draw() }), pct)),
      el("div", { class: "fld" }, el("label", {}, t("editor.sounds.priority")),
        el("input", { type: "number", min: 0, max: 255, value: e.priority,
          onchange: (x) => { e.priority = Math.max(0, Math.min(255, Number(x.target.value) || 0)); draw(); } }),
        el("p", { class: "hint" }, t("editor.sounds.priority_hint"))),
      interfaceOnly ? null : el("label", { class: "check" },
        el("input", { type: "checkbox", checked: e.positional, onchange: (x) => { e.positional = x.target.checked; draw(); } }),
        t("editor.sounds.positional")));
  }
  return card;
}

function techniquesPart() {
  const part = el("div", { id: "snd-techniques" }, el("h2", {}, t("editor.sounds.techniques")),
    el("p", { class: "hint" }, t("editor.sounds.techniques_hint")));
  for (const id of Object.keys(SND.cfg.techniques).sort()) {
    const known = SND.info.techniques.includes(id);
    part.append(bindingCard(SND.cfg.techniques, id, id, known ? t("editor.sounds.positional_hint") : t("editor.sounds.unknown_technique"),
      false, () => { delete SND.cfg.techniques[id]; draw(); }));
  }
  const free = SND.info.techniques.filter((id) => !SND.cfg.techniques[id]);
  const pick = el("select", {}, free.map((id) => el("option", { value: id }, id)));
  part.append(el("div", { class: "btnrow" }, pick,
    el("button", { class: "ghost", disabled: !free.length || !soundNames().length,
      onclick: () => { SND.cfg.techniques[pick.value] = { sounds: [soundNames()[0]], volume: 1000, priority: 120, positional: true }; draw(); jump("bind-" + pick.value); } },
    icon("plus", 16), t("editor.sounds.add_technique"))));
  return part;
}

// P05: music of the screens outside the world (login, selection, creation).
function screensPart() {
  const part = el("div", { id: "snd-screens" }, el("h2", {}, t("editor.sounds.screens")),
    el("p", { class: "hint" }, t("editor.sounds.screens_hint")));
  const names = soundNames();
  for (const screen of SND.info.screens) {
    const z = SND.cfg.screens[screen];
    const card = el("section", { class: "card", id: "screen-" + screen });
    card.append(el("div", { class: "lang-head" }, el("div", { class: "grow" }, el("strong", {}, t("editor.sounds.screen." + screen)))));
    const choose = (name) => {
      if (!name) delete SND.cfg.screens[screen];
      else SND.cfg.screens[screen] = { music: name, volume: z?.volume ?? 700 };
      draw();
    };
    card.append(el("div", { class: "fld" }, el("label", {}, t("editor.sounds.music")),
      el("div", { class: "btnrow" },
        el("select", { onchange: (x) => choose(x.target.value) },
          el("option", { value: "", selected: !z }, "— " + t("editor.sounds.no_music")),
          names.map((n) => el("option", { value: n, selected: z?.music === n }, n))),
        el("button", { class: "ghost sm", disabled: !z, title: t("editor.sounds.play"), onclick: () => playSound(soundFile(z.music), z.volume / 1000) }, "▶"),
        el("button", { class: "ghost sm", title: t("editor.sounds.stop"), onclick: stopSound }, "■"),
        el("button", { class: "ghost sm", onclick: () => uploadSound(choose, "music", screen) }, "⇪ ", t("editor.sounds.upload_use")))));
    if (z) {
      const pct = el("span", { class: "hint mono" }, Math.round(z.volume / 10) + "%");
      card.append(el("div", { class: "fld" }, el("label", {}, t("editor.sounds.volume")),
        el("div", { class: "btnrow" },
          el("input", { type: "range", min: 0, max: 1000, step: 10, value: z.volume,
            oninput: (x) => { z.volume = Number(x.target.value); pct.textContent = Math.round(z.volume / 10) + "%"; },
            onchange: () => draw() }), pct)));
    }
    part.append(card);
  }
  return part;
}

function zonesPart() {
  const part = el("div", { id: "snd-zones" }, el("h2", {}, t("editor.sounds.zones")),
    el("p", { class: "hint" }, t("editor.sounds.zones_hint")));
  const names = soundNames();
  const zones = [...new Set([...SND.info.zones, ...Object.keys(SND.cfg.zones)])].sort();
  for (const zone of zones) {
    const z = SND.cfg.zones[zone];
    const card = el("section", { class: "card", id: "zone-" + zone });
    card.append(el("div", { class: "lang-head" }, el("div", { class: "grow" }, el("strong", {}, zone),
      SND.info.zones.includes(zone) ? null : el("div", { class: "hint warnline" }, t("editor.sounds.unknown_zone")))));
    const choose = (name) => {
      if (!name) delete SND.cfg.zones[zone];
      else SND.cfg.zones[zone] = { music: name, volume: z?.volume ?? 700, fade: z?.fade ?? SND.info.default_fade };
      draw();
    };
    card.append(el("div", { class: "fld" }, el("label", {}, t("editor.sounds.music")),
      el("div", { class: "btnrow" },
        el("select", { onchange: (x) => choose(x.target.value) },
          el("option", { value: "", selected: !z }, "— " + t("editor.sounds.no_music")),
          names.map((n) => el("option", { value: n, selected: z?.music === n }, n))),
        el("button", { class: "ghost sm", disabled: !z, title: t("editor.sounds.play"), onclick: () => playSound(soundFile(z.music), z.volume / 1000) }, "▶"),
        el("button", { class: "ghost sm", title: t("editor.sounds.stop"), onclick: stopSound }, "■"),
        el("button", { class: "ghost sm", onclick: () => uploadSound(choose, "music", zone) }, "⇪ ", t("editor.sounds.upload_use")))));
    if (z) {
      const pct = el("span", { class: "hint mono" }, Math.round(z.volume / 10) + "%");
      card.append(
        el("div", { class: "fld" }, el("label", {}, t("editor.sounds.volume")),
          el("div", { class: "btnrow" },
            el("input", { type: "range", min: 0, max: 1000, step: 10, value: z.volume,
              oninput: (x) => { z.volume = Number(x.target.value); pct.textContent = Math.round(z.volume / 10) + "%"; },
              onchange: () => draw() }), pct)),
        el("div", { class: "fld" }, el("label", {}, t("editor.sounds.fade")),
          el("input", { type: "number", min: 0, max: 255, value: z.fade,
            onchange: (x) => { z.fade = Math.max(0, Math.min(255, Number(x.target.value) || 0)); draw(); } }),
          el("p", { class: "hint" }, t("editor.sounds.fade_hint"))));
    }
    part.append(card);
  }
  return part;
}

function libraryCard() {
  const card = el("section", { class: "card", id: "snd-library" }, el("h2", {}, t("editor.sounds.library")));
  const names = soundNames();
  if (!names.length) card.append(el("p", { class: "hint" }, t("editor.sounds.no_files")));
  for (const name of names) {
    const s = SND.cfg.sounds[name];
    const used = soundUsers(name);
    card.append(el("div", { class: "card lang" },
      el("div", { class: "lang-head" },
        el("strong", { class: "grow mono" }, name),
        el("button", { class: "ghost sm", title: t("editor.sounds.play"), onclick: () => playSound(s.file) }, "▶"),
        el("button", { class: "ghost sm", title: t("editor.sounds.stop"), onclick: stopSound }, "■"),
        el("button", { class: "ghost sm danger", disabled: used.length > 0,
          title: used.length ? t("editor.sounds.in_use") + ": " + used.join(", ") : t("editor.sounds.remove"),
          onclick: () => { delete SND.cfg.sounds[name]; draw(); } }, "✕")),
      el("div", { class: "fld" }, el("label", {}, t("editor.sounds.file")),
        el("select", { onchange: (x) => { s.file = x.target.value; draw(); } },
          SND.info.files.map((f) => el("option", { value: f, selected: f === s.file }, f)))),
      el("div", { class: "fld" }, el("label", {}, t("editor.sounds.license")),
        el("select", { onchange: (x) => { s.license = x.target.value; draw(); } },
          el("option", { value: "", selected: !s.license }, "—"),
          SND.info.licenses.map((l) => el("option", { value: l, selected: l === s.license }, l)))),
      el("div", { class: "fld" }, el("label", {}, t("editor.sounds.credit")),
        el("input", { value: s.credit, onchange: (x) => { s.credit = x.target.value; draw(); } }))));
  }
  card.append(el("div", { class: "btnrow" },
    el("button", { class: "ghost", onclick: () => uploadSound() }, "⇪ ", t("editor.sounds.upload"))));
  const loose = SND.info.files.filter((f) => !Object.values(SND.cfg.sounds).some((s) => s.file === f));
  if (loose.length) {
    card.append(el("h3", {}, t("editor.sounds.unregistered")));
    for (const f of loose) {
      card.append(el("div", { class: "btnrow" }, el("code", { class: "grow" }, f),
        el("button", { class: "ghost sm", onclick: () => playSound(f) }, "▶"),
        el("button", { class: "ghost sm", onclick: () => { register(f); draw(); } }, t("editor.sounds.register"))));
    }
  }
  return card;
}

// A file of the list under a free name made from its own; the credit
// starts empty and must be filled before saving.
function register(file, license = "") {
  const base = file.split("/").pop().replace(/\.ogg$/, "");
  let name = base, n = 2;
  while (SND.cfg.sounds[name]) name = `${base}_${n++}`;
  SND.cfg.sounds[name] = { file, license, credit: "" };
  return name;
}

// Uploads one .ogg and adds it to the list; `use` receives the new sound's
// name (to link it to an event, technique or zone at once).
async function uploadSound(use, folderHint, nameHint) {
  const file = el("input", { type: "file", accept: ".ogg,audio/ogg" });
  const folders = [...new Set([...SND.info.folders, "music", "combat", "ui", "technique"])];
  const folder = el("input", { class: "mono", list: "sound-folders", value: folderHint || SND.info.folders[0] || "combat" });
  const name = el("input", { class: "mono", value: nameHint || "" });
  const license = el("select", {}, SND.info.licenses.map((l) => el("option", { value: l }, l)));
  const credit = el("input", { placeholder: t("editor.sounds.credit") });
  file.addEventListener("change", () => {
    const f = file.files[0];
    if (f && !name.value) name.value = slug(f.name.replace(/\.[^.]+$/, ""));
  });
  const ok = await dialog(t("editor.sounds.upload"), [
    el("datalist", { id: "sound-folders" }, folders.map((f) => el("option", { value: f }))),
    el("div", { class: "fld" }, el("label", {}, t("editor.sounds.file")), file),
    el("div", { class: "fld" }, el("label", {}, t("editor.sounds.folder")), folder,
      el("p", { class: "hint" }, t("editor.sounds.folder_hint"))),
    el("div", { class: "fld" }, el("label", {}, t("editor.sounds.name")), name),
    el("div", { class: "fld" }, el("label", {}, t("editor.sounds.license")), license),
    el("div", { class: "fld" }, el("label", {}, t("editor.sounds.credit")), credit),
    el("p", { class: "hint" }, t("editor.sounds.convert_hint")),
  ], [
    { label: t("editor.cancel"), value: false },
    { label: t("editor.sounds.upload"), value: true, primary: true },
  ], { validate: () => !!file.files[0] && /^[a-z0-9_]+$/.test(folder.value) && /^[a-z0-9_]+$/.test(name.value) });
  if (!ok) return;
  const f = file.files[0];
  if (f.size > SND.info.max_bytes) { await say(t("editor.sounds.upload"), `> ${SND.info.max_bytes} bytes`); return; }
  const replace = SND.info.files.includes(folder.value === "music" ? `music/${name.value}.ogg` : `sounds/${folder.value}/${name.value}.ogg`);
  try {
    const r = await api(`/api/sound-file/${folder.value}/${name.value}${replace ? "?replace=true" : ""}`, f, true);
    const keep = SND.cfg;
    await loadSounds();
    SND.cfg = keep;
    let sound = Object.entries(SND.cfg.sounds).find(([, s]) => s.file === r.path)?.[0];
    if (!sound) sound = register(r.path, license.value);
    if (credit.value.trim()) SND.cfg.sounds[sound].credit = credit.value.trim();
    if (!SND.cfg.sounds[sound].license) SND.cfg.sounds[sound].license = license.value;
    if (use) use(sound);
    toast(t("editor.sounds.uploaded"));
  } catch (e) {
    await say(t("editor.sounds.upload"), e.message);
  }
  draw();
}

function drawSounds() {
  const p = $("panel");
  p.replaceChildren();
  const page = el("div", { class: "page" });
  const dirtyNow = soundsDirty();
  page.append(el("header", { class: "page-head" },
    el("div", { class: "head-txt" }, el("div", { class: "crumb" }, "assets/sounds/sounds.ron"), el("h1", {}, t("editor.sounds"))),
    el("div", { class: "grow" }),
    el("button", { class: "ghost", onclick: () => uploadSound() }, "⇪ ", t("editor.sounds.upload")),
    el("button", { class: "ghost", disabled: !dirtyNow, onclick: () => { SND.cfg = JSON.parse(SND.saved); draw(); } }, t("editor.looks.discard")),
    el("button", { class: "primary", disabled: !dirtyNow, onclick: saveSounds }, icon("check", 16), t("editor.save"))));
  page.append(el("p", { class: "hint" }, t("editor.sounds.intro")));
  if (SND.info.warnings.length) {
    page.append(el("section", { class: "card" }, el("h3", {}, t("editor.sounds.missing")),
      SND.info.warnings.map((w) => el("p", { class: "hint warnline" }, w))));
  }
  const events = el("div", { id: "snd-events" }, el("h2", {}, t("editor.sounds.events")));
  for (const ev of SND.info.events) {
    events.append(bindingCard(SND.cfg.events, ev.id, evLabel(ev.id),
      t(ev.interface ? "editor.sounds.interface_hint" : "editor.sounds.positional_hint"), ev.interface));
  }
  page.append(events, techniquesPart(), zonesPart(), screensPart(), libraryCard());
  p.append(page);
}
