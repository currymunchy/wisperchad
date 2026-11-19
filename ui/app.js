// wisperChad main window. Plain JS on purpose: no build step, easy to hack on.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const MEASURED_KEY = "wisperchad.measured";
const HISTORY_KEY = "wisperchad.history";
const HISTORY_LIMIT = 10;
const DEFAULT_HOTKEYS = navigator.userAgent.includes("Windows")
  ? { dictate_hotkey: "Ctrl+Win", reply_hotkey: "Ctrl+Win+Alt" }
  : navigator.userAgent.includes("Macintosh")
    ? { dictate_hotkey: "Ctrl+Alt+D", reply_hotkey: "Ctrl+Alt+R" }
    : { dictate_hotkey: "Ctrl+Alt+Space", reply_hotkey: "Ctrl+Alt+R" };

const state = {
  settings: null,
  catalog: { stt: [], llm: [], featured: [] },
  hardware: null,
  presets: [],
  ollama: { running: false, installed: [], sizes: {}, catalogInstalled: [] },
  openReq: new Set(),
  downloading: {}, // id -> {done,total} or {status,completed,total}
  measured: loadMeasured(), // id -> {total_bytes,vram_bytes} or {error}
  measuring: new Set(),
  history: loadHistory(),
};

function loadHistory() {
  try {
    return JSON.parse(localStorage.getItem(HISTORY_KEY)) || [];
  } catch {
    return [];
  }
}

function saveHistory() {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(state.history));
  } catch {
    // Only costs the list on the next launch.
  }
}

function loadMeasured() {
  try {
    return JSON.parse(localStorage.getItem(MEASURED_KEY)) || {};
  } catch {
    return {};
  }
}

function saveMeasured() {
  try {
    localStorage.setItem(MEASURED_KEY, JSON.stringify(state.measured));
  } catch {
    // Only costs a re-measure next time.
  }
}

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const fmtMB = (mb) => (mb >= 1000 ? (mb / 1000).toFixed(1) + " GB" : mb + " MB");
const fmtBytes = (b) => (b >= 1e9 ? (b / 1e9).toFixed(2) + " GB" : (b / 1e6).toFixed(0) + " MB");
const fmtGb = (b) => (b / 1e9).toFixed(1) + " GB";
const dots = (n) => "●".repeat(n) + "○".repeat(5 - n);

const ON_MAC = navigator.userAgent.includes("Macintosh");
const ON_LINUX = navigator.userAgent.includes("Linux");
const SHORTCUT_KEY = ON_MAC ? "Cmd" : "Ctrl";
// Settings always say Ctrl, Alt and Win. This puts those in the words on this keyboard.
const keyNames = (text) => {
  if (ON_MAC) return text.replace(/\bWin\b/g, "Cmd").replace(/\bAlt\b/g, "Option");
  return ON_LINUX ? text.replace(/\bWin\b/g, "Super") : text;
};

function toast(msg, ms = 3500) {
  const t = $("#toast");
  t.textContent = msg;
  t.classList.remove("hidden");
  clearTimeout(toast._t);
  toast._t = setTimeout(() => t.classList.add("hidden"), ms);
}

// ------------------------------------------------------------ navigation
document.querySelectorAll(".nav").forEach((b) =>
  b.addEventListener("click", () => showView(b.dataset.view))
);
function showView(name) {
  document.querySelectorAll(".nav").forEach((x) => x.classList.toggle("active", x.dataset.view === name));
  document.querySelectorAll(".view").forEach((v) => v.classList.toggle("active", v.id === "view-" + name));
  document.querySelector("main").scrollTop = 0;
  refreshOllama(); // keep install status fresh when moving around
}

// ------------------------------------------------------------ requirements check
function bestGpu() {
  const g = state.hardware?.gpus || [];
  return g.length ? g.reduce((a, b) => (b.vram_gb > a.vram_gb ? b : a)) : null;
}

const usesOllama = () => state.settings.llm_engine === "ollama";

// Whether a writing model is ready to use in the engine the user has.
function llmInstalled(m) {
  if (!usesOllama()) return !!m.installed;
  return state.ollama.catalogInstalled.includes(m.id) || state.ollama.installed.includes(m.id);
}

// What a writing model really used when loaded on this PC, with the button that finds out.
function measuredRows(id) {
  const m = state.measured[id];
  const button = state.measuring.has(id)
    ? `<button class="btn" disabled>Measuring… loading the model</button>`
    : `<button class="btn" data-act="measure">${m ? "Measure again" : "Measure on my PC"}</button>`;
  let rows = "";
  if (m?.error) {
    rows = `<span class="k">On your PC</span><span class="verdict fail">✗ Couldn't load</span><span></span>
      <div class="note">${esc(m.error)}</div>`;
  } else if (m) {
    const spill = m.total_bytes - m.vram_bytes;
    const fits = m.vram_bytes > 0 && spill < 1e8;
    const verdict = fits ? "✓ Fits on your GPU" : m.vram_bytes ? `⚠ ${fmtGb(spill)} spills to RAM, so it's slower` : "⚠ Runs on the CPU, so it's slower";
    rows = `<span class="k">On your PC</span><span>${fmtGb(m.total_bytes)} used, ${fmtGb(m.vram_bytes)} on the GPU</span>
      <span class="${fits ? "pass" : "warn"}">${verdict}</span>`;
  }
  return `${rows}<div class="req-actions">${button}</div>`;
}

function reqRows(req, kind, id, installed) {
  const hw = state.hardware;
  if (!hw) return "";
  const measure = kind === "llm" && installed && usesOllama() ? measuredRows(id) : "";
  if (req.unknown) {
    return `
      <div class="req">
        <span class="k">Memory (RAM)</span><span>Not listed by Ollama</span><span></span>
        <span class="k">GPU memory (VRAM)</span><span>Not listed by Ollama</span><span></span>
        <span class="k">Disk space</span><span>${req.disk_mb ? fmtMB(req.disk_mb) : "Unknown"}</span><span></span>
        ${measure}
        <div class="note">${esc(req.note)}</div>
      </div>`;
  }
  const { ramOk, gpuCell, gpuClass, verdict, vclass } = fit(req, kind);

  return `
    <div class="req">
      <span class="k">Memory (RAM)</span><span>${req.min_ram_gb} GB minimum</span>
      <span class="${ramOk ? "pass" : "fail"}">${ramOk ? "✓" : "✗"} you have ${hw.ram_gb.toFixed(0)} GB</span>
      <span class="k">GPU memory (VRAM)</span><span>${req.recommended_vram_gb ? req.recommended_vram_gb + " GB recommended" : "Optional"}</span>
      <span class="${gpuClass}">${gpuClass === "pass" ? "✓" : "⚠"} ${esc(gpuCell)}</span>
      <span class="k">Disk space</span><span>${fmtMB(req.disk_mb)}</span><span></span>
      ${measure && state.measured[id] ? "" : `<span class="k">Verdict</span><span class="verdict ${vclass}">${verdict}</span><span></span>`}
      ${measure}
      <div class="note">${esc(req.note)}</div>
    </div>`;
}

// How a model should run on this PC.
function fit(req, kind) {
  const hw = state.hardware;
  const gpu = bestGpu();
  const ramOk = hw.ram_gb + 0.25 >= req.min_ram_gb;
  let gpuCell, gpuClass;
  if (req.recommended_vram_gb === 0) {
    gpuCell = "Not needed";
    gpuClass = "pass";
  } else if (!gpu) {
    gpuCell = "No graphics card found: may use the CPU (slower)";
    gpuClass = "warn";
  } else if (kind === "stt" && !hw.gpu_build) {
    gpuCell = ON_MAC || ON_LINUX
      ? `${gpu.name}, but speech runs on the CPU on this system`
      : `${gpu.name}, but this is the CPU-only installer. Get the GPU installer for speed`;
    gpuClass = "warn";
  } else if (gpu.vram_gb + 0.25 >= req.recommended_vram_gb) {
    gpuCell = `${gpu.name} (${gpu.vram_gb.toFixed(0)} GB)`;
    gpuClass = "pass";
  } else {
    gpuCell = `${gpu.name} (${gpu.vram_gb.toFixed(0)} GB): may spill to RAM and slow down`;
    gpuClass = "warn";
  }
  let verdict, vclass;
  if (!ramOk) { verdict = "✗ Not enough RAM"; vclass = "fail"; }
  else if (gpuClass === "warn") { verdict = "⚠ Will run, maybe slowly"; vclass = "warn"; }
  else { verdict = "✓ Runs well on your PC"; vclass = "pass"; }
  return { ramOk, gpuCell, gpuClass, verdict, vclass };
}

// ------------------------------------------------------------ model lists
function progressHtml(id) {
  const d = state.downloading[id];
  if (!d) return "";
  const done = d.done ?? d.completed ?? 0;
  const total = d.total || 0;
  const pct = total ? Math.min(100, (done / total) * 100) : 0;
  const label = total ? `${fmtBytes(done)} of ${fmtBytes(total)}` : esc(d.status || "Starting…");
  return `<div class="progress"><div style="width:${pct}%"></div></div><div class="progress-label">${label}</div>`;
}

// Download events arrive many times a second. Touching only the progress bar keeps the rest of
// the list in place: redrawing it would swallow clicks on every other model.
function updateProgress(id) {
  document.querySelectorAll(`.model[data-id="${CSS.escape(id)}"] .progress-slot`).forEach((slot) => (slot.innerHTML = progressHtml(id)));
}

function modelCard({ id, name, description, languages, speed, quality, recommended, requirements, installed, inUse, kind, size, detailed }) {
  const busy = !!state.downloading[id];
  const fits = state.hardware && !requirements.unknown && !installed ? fit(requirements, kind) : null;
  const actions = busy
    ? `<button class="btn" disabled>Installing…</button>`
    : installed
      ? `${inUse
          // Turning the writing model off leaves wisperChad typing just what was said.
          ? kind === "llm" ? `<button class="btn" data-act="stop">Stop using</button>` : ""
          : `<button class="btn primary" data-act="use">Use</button>`}
         <button class="btn danger" data-act="remove">Remove</button>`
      : `<button class="btn" data-act="install">Install${size ? " · " + fmtMB(size) : ""}</button>`;
  const open = detailed || state.openReq.has(kind + id);
  return `
    <div class="model ${inUse && installed ? "selected" : ""}" data-id="${esc(id)}" data-kind="${kind}">
      <div class="model-top">
        <div class="model-info">
          <div class="model-name">${esc(name)}
            ${recommended ? `<span class="badge rec">Recommended</span>` : ""}
            ${inUse && installed ? `<span class="badge inuse">In use</span>` : ""}
            ${inUse && !installed && !busy ? `<span class="badge missing">Selected but not installed</span>` : ""}
            ${!inUse && installed ? `<span class="badge">Installed</span>` : ""}
            ${fits ? `<span class="badge ${fits.vclass}">${fits.verdict}</span>` : ""}
          </div>
          <div class="model-desc">${esc(description)}</div>
          <div class="meta">
            ${speed ? `<span>Speed <span class="dots">${dots(speed)}</span></span>` : ""}
            ${quality ? `<span>Quality <span class="dots">${dots(quality)}</span></span>` : ""}
            ${languages ? `<span>${esc(languages)}</span>` : ""}
            ${size ? `<span>${fmtMB(size)}</span>` : ""}
          </div>
        </div>
        <div class="actions">${actions}</div>
      </div>
      ${detailed ? "" : `<button class="req-toggle" data-act="req">${open ? "Hide requirements ▴" : "Requirements ▾"}</button>`}
      ${open ? reqRows(requirements, kind, id, installed) : ""}
      <div class="progress-slot">${progressHtml(id)}</div>
    </div>`;
}

// The writing models this engine can run, including anything else already in Ollama.
function writingModels() {
  if (!usesOllama()) return state.catalog.llm.filter((m) => m.files.length);
  const known = new Set(state.catalog.llm.map((m) => m.id));
  const extra = state.ollama.installed
    .filter((t) => !known.has(t) && !known.has(t.replace(/:latest$/, "")))
    .map((t) => ({
      id: t, name: t, description: "Installed in Ollama.", recommended: false,
      requirements: {
        unknown: true,
        disk_mb: Math.round((state.ollama.sizes?.[t] || 0) / 1e6),
        note: "Ollama doesn't list memory needs, and download size is a poor guide. Measure it to see what it really uses on your PC.",
      },
    }));
  return [...state.catalog.llm, ...extra];
}

// The preset the models in use make up. Models picked by hand may not be one.
function presetInUse() {
  const s = state.settings;
  return state.presets.find((p) => p.speech_model === s.stt_model && (p.writing_model ?? null) === (s.llm_model ?? null)) ?? null;
}

function renderModels() {
  const s = state.settings;
  const ollama = usesOllama();
  const preset = presetInUse();
  $("#preset-in-use").innerHTML = s.stt_model && state.presets.length ? `Current preset: <b>${esc(preset?.name ?? "Custom")}</b>` : "";
  $("#custom-model").classList.toggle("hidden", !ollama);
  const banner = $("#ollama-banner");
  if (!ollama) {
    banner.innerHTML = state.ollama.running
      ? `<div class="banner"><b>Ollama is running on this PC.</b> wisperChad can use it and the models already in it,
         so nothing downloads twice. <a data-engine="ollama">Use Ollama</a></div>`
      : "";
  } else {
    banner.innerHTML = state.ollama.running
      ? `<div class="banner">Writing models run through Ollama. <a data-engine="builtin">Use the built-in engine instead</a></div>`
      : `<div class="banner warn"><b>Ollama isn't running.</b> Open it, then <a id="recheck">check again</a>,
         or <a data-engine="builtin">use the built-in engine</a>, which needs nothing else installed.</div>`;
  }
  $("#recheck")?.addEventListener("click", refreshOllama);
  banner.querySelectorAll("[data-engine]").forEach((a) => a.addEventListener("click", () => setEngine(a.dataset.engine)));

  const stt = state.catalog.stt.map((m) => ({ ...m, size: m.requirements.disk_mb, kind: "stt", inUse: s.stt_model === m.id }));
  const llm = writingModels()
    .map((m) => ({ ...m, size: m.requirements.disk_mb, kind: "llm", installed: llmInstalled(m), inUse: s.llm_model === m.id }));
  // The main page stays at two of each, plus whatever is in use from the More models tab.
  const featured = new Set(state.catalog.featured);
  const main = (list) => list.filter((m) => featured.has(m.id) || m.inUse).sort((a, b) => b.recommended - a.recommended).map((m) => modelCard(m)).join("");
  const more = (list) => list.filter((m) => !featured.has(m.id)).map((m) => modelCard({ ...m, detailed: true })).join("");
  $("#stt-list").innerHTML = main(stt);
  $("#llm-list").innerHTML = main(llm);
  $("#more-stt-list").innerHTML = more(stt);
  $("#more-llm-list").innerHTML = more(llm);
  if (ollama) document.querySelectorAll("[data-kind='llm'] button[data-act='install']").forEach((b) => (b.disabled = !state.ollama.running));
}

async function setEngine(engine) {
  state.settings = { ...state.settings, llm_engine: engine };
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 6000));
  fillSettings();
  await refreshAll();
}

document.addEventListener("click", async (e) => {
  const btn = e.target.closest("button[data-act]");
  if (!btn) return;
  const card = btn.closest(".model");
  const { id, kind } = card.dataset;
  const act = btn.dataset.act;
  if (act === "req") {
    const key = kind + id;
    state.openReq.has(key) ? state.openReq.delete(key) : state.openReq.add(key);
    return renderModels();
  }
  if (act === "measure") {
    state.measuring.add(id);
    renderModels();
    try {
      state.measured[id] = await invoke("measure_llm", { tag: id });
    } catch (err) {
      state.measured[id] = { error: String(err) };
    }
    state.measuring.delete(id);
    saveMeasured();
    return renderModels();
  }
  try {
    if (act === "install") await install(kind, id);
    if (act === "use") {
      state.settings[kind === "stt" ? "stt_model" : "llm_model"] = id;
      await invoke("save_settings", { settings: state.settings });
      toast(`Now using ${id}`);
    }
    if (act === "stop") {
      state.settings.llm_model = null;
      await invoke("save_settings", { settings: state.settings });
      toast("Writing model off: wisperChad types just what you say, with no clean-up or replies");
    }
    if (act === "remove") {
      if (kind === "stt") await invoke("delete_stt", { id });
      else await invoke("delete_llm", { tag: id });
      toast("Removed");
    }
  } catch (err) {
    toast(String(err), 6000);
  }
  await refreshAll();
});

async function install(kind, id) {
  state.downloading[id] = {};
  renderModels();
  try {
    if (kind === "stt") await invoke("download_stt", { id });
    else await invoke("install_llm", { tag: id });
    toast(`${id} installed`);
  } finally {
    delete state.downloading[id];
  }
}

$("#custom-pull").addEventListener("click", async () => {
  const tag = $("#custom-tag").value.trim();
  if (!tag) return;
  const bar = $("#custom-progress");
  bar.classList.remove("hidden");
  state.downloading[tag] = {};
  const unlisten = await listen("llm-pull", ({ payload }) => {
    if (payload.id !== tag) return;
    const p = payload.progress;
    bar.firstElementChild.style.width = p.total ? (100 * (p.completed || 0)) / p.total + "%" : "5%";
  });
  try {
    await invoke("install_llm", { tag });
    toast(`${tag} installed`);
    $("#custom-tag").value = "";
  } catch (err) {
    toast(String(err), 6000);
  } finally {
    unlisten();
    delete state.downloading[tag];
    bar.classList.add("hidden");
    refreshAll();
  }
});

listen("model-download", ({ payload }) => {
  state.downloading[payload.id] = { done: payload.done, total: payload.total };
  updateProgress(payload.id);
});
listen("llm-pull", ({ payload }) => {
  state.downloading[payload.id] = payload.progress;
  updateProgress(payload.id);
});

// ------------------------------------------------------------ home
// Replies need a writing model, so until there is one the hotkey shows as empty. It stays saved for
// when one is added.
function renderReplyHotkey() {
  const hotkey = state.settings.llm_model ? keyNames(state.settings.reply_hotkey) : "";
  document.querySelectorAll("[data-hotkey='reply']").forEach((e) => {
    e.textContent = hotkey;
    e.classList.toggle("hidden", !hotkey);
  });
  const input = $("#set-reply");
  if (input === document.activeElement) return; // It's waiting for a new hotkey.
  input.value = hotkey;
  input.disabled = !hotkey;
  input.placeholder = hotkey ? "" : "Needs a writing model";
}

function renderHome() {
  const s = state.settings;
  document.querySelectorAll("[data-hotkey='dictate']").forEach((e) => (e.textContent = keyNames(s.dictate_hotkey)));
  renderReplyHotkey();
  const hold = (s.hotkey_style || "hold") === "hold";
  $("#dictate-how").textContent = hold
    ? "Hold, speak, release. Or double-press to keep listening, and press once more to stop."
    : "Press once, speak, press again. Filler words and punctuation get cleaned up.";
  $("#reply-how-1").innerHTML = s.reply_source === "screen"
    ? "Open the email or message so it's on screen"
    : `Select the email or message and press <kbd>${SHORTCUT_KEY}</kbd>+<kbd>C</kbd>`;
  $("#reply-off").classList.toggle("hidden", !!s.llm_model);
  $("#reply-steps").classList.toggle("hidden", !s.llm_model);
  $("#reply-how-3").textContent = hold
    ? "Hold the hotkey, or double-press it, and say what you want to say, however rough"
    : "Press the hotkey, say what you want to say, press it again";
  const sttReady = s.stt_model && state.catalog.stt.find((m) => m.id === s.stt_model)?.installed;
  const selected = state.catalog.llm.find((m) => m.id === s.llm_model) ?? { id: s.llm_model };
  const items = [
    [sttReady, "Speech model installed", "Install a speech model"],
    ...(usesOllama() ? [[state.ollama.running, "Ollama is running", "Open Ollama, or use the built-in engine"]] : []),
    [!!s.llm_model && llmInstalled(selected), "Writing model installed", "Add a writing model for clean-up and replies (optional)"],
  ];
  $("#checklist").innerHTML = items
    .map(([ok, done, todo]) => `<li><span class="dot ${ok ? "ok" : "todo"}">${ok ? "✓" : "○"}</span>
      ${ok ? esc(done) : `<a data-goto="models">${esc(todo)}</a>`}</li>`)
    .join("");
  document.querySelectorAll("[data-goto]").forEach((a) => a.addEventListener("click", () => showView(a.dataset.goto)));

  $("#history").innerHTML = state.history.length
    ? state.history
        .map((h, i) => `<div class="item">
          <button class="history-delete" data-history="${i}" title="Delete this chat">Delete</button>
          <div class="heard">${h.mode === "reply" ? "Reply" : "Dictation"} · heard: “${esc(h.heard)}”</div>
          <div class="out">${esc(h.output)}</div></div>`)
        .join("")
    : `<p class="muted">Nothing yet. Try the hotkey.</p>`;
}

listen("history", ({ payload }) => {
  state.history = [payload, ...state.history].slice(0, HISTORY_LIMIT);
  saveHistory();
  renderHome();
});

$("#history").addEventListener("click", (e) => {
  const button = e.target.closest("[data-history]");
  if (!button) return;
  state.history.splice(Number(button.dataset.history), 1);
  saveHistory();
  renderHome();
});

function renderPc() {
  const hw = state.hardware;
  if (!hw) return;
  const gpu = bestGpu();
  $("#pc-summary").textContent =
    `${hw.ram_gb.toFixed(0)} GB RAM · ${hw.cpu_cores} threads\n` +
    (gpu ? `${gpu.name.replace("NVIDIA GeForce ", "")} · ${gpu.vram_gb.toFixed(0)} GB VRAM` : "No graphics card detected") +
    (hw.gpu_build ? "\nGPU build" : "");
}

// ------------------------------------------------------------ settings
document.querySelectorAll("[data-keys]").forEach((e) => {
  e.textContent = keyNames(e.textContent).replace("Ctrl+C", `${SHORTCUT_KEY}+C`);
});

// A Mac shares the text of web pages, like an email in Safari, Chrome or Mail, through Accessibility.
// On Linux a reply goes by the screenshot alone.
if (ON_MAC) {
  $("#reply-source-help").textContent = "Reads the text of web pages and Mail, even parts scrolled out of view, using the Accessibility permission, and takes a screenshot for pictures and for other apps. Keep any picture on screen when you press the hotkey. Other apps need a writing model that can read images, like Qwen3.5 4B. It all goes only to the writing model on your Mac and is never saved.";
}
if (ON_LINUX) {
  $("#reply-from-screen").textContent = "The open window (screenshot)";
  $("#reply-source-help").textContent = "Takes a screenshot of the window, so keep the message and any picture on screen when you press the hotkey. This needs a writing model that can read images, like Qwen3.5 4B. It goes only to the writing model on your computer and is never saved.";
}
if (ON_LINUX) $("#set-mute").closest(".field").classList.add("hidden");

invoke("uses_wayland").then((wayland) => {
  if (!wayland) return;
  $("#wayland-banner").innerHTML = `<div class="banner warn"><b>This desktop runs on Wayland.</b> It keeps apps from
    using hotkeys everywhere or typing for you, so your text lands on the clipboard: press <kbd>Ctrl</kbd>+<kbd>V</kbd>
    to paste it. For a hotkey that works everywhere, add a shortcut in your desktop's settings that runs
    <kbd>wisperchad --dictate</kbd> or <kbd>wisperchad --reply</kbd>, or log in with an X11 session.</div>`;
});

function fillSettings() {
  const s = state.settings;
  $("#set-theme").value = savedTheme();
  $("#set-dictate").value = keyNames(s.dictate_hotkey);
  renderReplyHotkey();
  $("#set-style").value = s.hotkey_style || "hold";
  $("#set-mic").value = s.microphone;
  $("#set-mute").checked = s.mute_while_recording;
  $("#set-auto-submit").checked = s.auto_submit;
  $("#set-launch").value = s.launch_on_startup || "off";
  $("#set-reply-source").value = s.reply_source || "clipboard";
  $("#set-tone").value = s.tone;
  $("#set-name").value = s.user_name;
  $("#set-cleanup").checked = s.cleanup_enabled;
  $("#set-keepalive").value = String(s.keep_alive_minutes);
  if ($("#set-keepalive").value === "") $("#set-keepalive").value = "5";
  $("#set-engine").value = s.llm_engine || "builtin";
  $("#set-ollama").value = s.ollama_url;
  refreshSelects();
}

// Click a hotkey box, press a combo: it's saved and active immediately. Global hotkeys are paused
// while a box is focused so the current hotkey can be re-typed. Setup draws boxes of its own, so
// this listens on the whole page, and each box says which hotkey it sets in data-field.
const hotkeyBox = (element) => (element?.classList?.contains("hotkey-input") ? element : null);

async function takeHotkey(input, combo) {
  const field = input.dataset.field;
  const other = field === "dictate_hotkey" ? "reply_hotkey" : "dictate_hotkey";
  if (combo === state.settings[other]) return toast("That's already the other hotkey");
  input.value = keyNames(combo);
  input.dataset.captured = "1";
  input.blur();
  await saveHotkey(field, combo);
  // In setup, straight on to trying it.
  if (!$("#setup").classList.contains("hidden")) $("#setup-box")?.focus();
}

// Modifiers held on their own have no final key press to catch here, so the app spots them.
listen("hotkey-chord", ({ payload }) => {
  const input = hotkeyBox(document.activeElement);
  if (input) takeHotkey(input, payload);
});
document.addEventListener("focusin", (e) => {
  const input = hotkeyBox(e.target);
  if (!input || input.classList.contains("recording")) return; // focus can fire twice
  input.dataset.captured = "";
  invoke("pause_hotkeys");
  input.classList.add("recording");
  input.value = "Press keys…";
});
document.addEventListener("focusout", (e) => {
  const input = hotkeyBox(e.target);
  if (!input) return;
  input.classList.remove("recording");
  if (input.value === "Press keys…") input.value = keyNames(state.settings[input.dataset.field]);
  // A captured combo is registered by saveHotkey; otherwise turn the old hotkeys back on.
  if (!input.dataset.captured) invoke("resume_hotkeys").catch((err) => toast(String(err), 6000));
});
document.addEventListener("keydown", async (e) => {
  const input = hotkeyBox(e.target);
  if (!input) return;
  e.preventDefault();
  if (["Control", "Alt", "Shift", "Meta", "OS"].includes(e.key)) return;
  if (!e.code) return; // The key that stops Start opening, which has no code of its own.
  if (e.key === "Escape") return input.blur();
  const mods = [e.ctrlKey && "Ctrl", e.metaKey && "Win", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
  const isFn = /^F\d+$/.test(e.code);
  if (!mods.length && !isFn) return toast(keyNames("Use at least one modifier (Ctrl, Alt, Shift, Win) or an F-key"));
  await takeHotkey(input, [...mods, e.code.replace(/^Key/, "").replace(/^Digit/, "")].join("+"));
});

async function saveHotkey(field, combo) {
  const before = state.settings[field];
  state.settings = { ...state.settings, [field]: combo };
  try {
    await invoke("save_settings", { settings: state.settings });
    toast(`Hotkey set to ${keyNames(combo)}`);
    document.querySelectorAll(`.hotkey-input[data-field="${field}"]`).forEach((input) => (input.value = keyNames(combo)));
  } catch (err) {
    toast(String(err), 6000); // e.g. another app already owns this combo
    state.settings = { ...state.settings, [field]: before };
    await invoke("save_settings", { settings: state.settings }).catch(() => {});
    fillSettings();
  }
  renderHome();
}

$("#reset-hotkeys").addEventListener("click", async () => {
  state.settings = { ...state.settings, ...DEFAULT_HOTKEYS, hotkey_style: "hold" };
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 6000));
  fillSettings();
  renderHome();
  toast("Hotkeys reset");
});

$("#set-style").addEventListener("change", async () => {
  state.settings = { ...state.settings, hotkey_style: $("#set-style").value };
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 6000));
  renderHome();
});

$("#set-theme").addEventListener("change", () => setTheme($("#set-theme").value));

async function saveChoice(key, value) {
  state.settings = { ...state.settings, [key]: value };
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 6000));
}

$("#set-mic").addEventListener("change", () => saveChoice("microphone", $("#set-mic").value));
$("#set-mute").addEventListener("change", () => saveChoice("mute_while_recording", $("#set-mute").checked));
$("#set-auto-submit").addEventListener("change", () => saveChoice("auto_submit", $("#set-auto-submit").checked));
$("#set-launch").addEventListener("change", () => saveChoice("launch_on_startup", $("#set-launch").value));

// An unplugged microphone stays listed, so the setting doesn't look like it was lost.
async function fillMicrophones() {
  const names = await invoke("list_microphones");
  const saved = state.settings.microphone;
  const all = saved && !names.includes(saved) ? [...names, saved] : names;
  all.forEach((name) => {
    const option = document.createElement("option");
    option.value = option.textContent = name;
    $("#set-mic").append(option);
  });
}
$("#set-engine").addEventListener("change", () => setEngine($("#set-engine").value));

$("#set-reply-source").addEventListener("change", async () => {
  state.settings = { ...state.settings, reply_source: $("#set-reply-source").value };
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 6000));
  renderHome();
});

$("#save-settings").addEventListener("click", async () => {
  const s = {
    ...state.settings,
    hotkey_style: $("#set-style").value,
    microphone: $("#set-mic").value,
    mute_while_recording: $("#set-mute").checked,
    auto_submit: $("#set-auto-submit").checked,
    launch_on_startup: $("#set-launch").value,
    reply_source: $("#set-reply-source").value,
    tone: $("#set-tone").value,
    user_name: $("#set-name").value,
    cleanup_enabled: $("#set-cleanup").checked,
    keep_alive_minutes: Number($("#set-keepalive").value),
    llm_engine: $("#set-engine").value,
    ollama_url: $("#set-ollama").value.replace(/\/+$/, ""),
  };
  if (s.dictate_hotkey === s.reply_hotkey) return toast("The two hotkeys must be different");
  try {
    await invoke("save_settings", { settings: s });
    $("#save-msg").textContent = "Saved ✓";
  } catch (err) {
    $("#save-msg").textContent = String(err);
  }
  setTimeout(() => ($("#save-msg").textContent = ""), 4000);
  await refreshAll();
});

// ------------------------------------------------------------ boot
async function refreshOllama() {
  state.ollama = await invoke("ollama_status");
  renderModels();
  renderHome();
}

async function refreshAll() {
  const [settings, catalog, ollama] = await Promise.all([
    invoke("get_settings"),
    invoke("get_catalog"),
    invoke("ollama_status"),
  ]);
  Object.assign(state, { settings, catalog, ollama });
  renderModels();
  renderHome();
}

// Waits for the page so setup.js has loaded too.
document.addEventListener("DOMContentLoaded", async () => {
  await refreshAll();
  await fillMicrophones();
  fillSettings();
  state.hardware = await invoke("get_hardware");
  state.presets = await invoke("get_presets", { languages: ["en"] });
  renderPc();
  renderModels();
  if (!state.settings.setup_done) openSetup();
  else if (!state.settings.stt_model) showView("models");
  renderAbout();
  setInterval(refreshOllama, 15000);
});
