// First-run setup: recommends a preset for the user's computer and installs it, then has them
// dictate and write a reply for real, so they know it all works before relying on it. It runs once,
// or again from Settings.
const SETUP_STEPS = ["preset", "install", "name", "allow", "dictate", "reply", "done"];

const setup = { step: "preset", presets: [], tab: "presets", speech: "", writing: "", error: "", status: "", statusClass: "", passed: false, trying: false, skipped: false, locked: false, tipShown: false, permissions: [] };

function writingInUse() {
  const id = state.settings.llm_model;
  if (!id) return null;
  const model = state.catalog.llm.find((m) => m.id === id) ?? { id, name: id };
  return llmInstalled(model) ? model : null;
}

const pickedSpeech = () => state.catalog.stt.find((m) => m.id === setup.speech);
const pickedWriting = () => writingModels().find((m) => m.id === setup.writing) ?? null;

const speechToInstall = () => (pickedSpeech().installed ? null : pickedSpeech());

function writingToInstall() {
  const model = pickedWriting();
  return model && !llmInstalled(model) ? model : null;
}

// From the Models page, setup is only the presets: pick one and it's switched, with no name, permissions
// or tests to go through again. Run setup again in Settings goes through all of it.
async function openSetup(presetsOnly = false) {
  Object.assign(setup, { error: "", skipped: false, presetsOnly: presetsOnly && state.settings.setup_done });
  // Ollama was running at first launch but has since closed: the built-in engine needs nothing.
  if (usesOllama() && !state.ollama.running && !writingInUse()) await setEngine("builtin");
  await showPresets();
  $("#setup").classList.remove("hidden");
}

// Says what will run the writing model, and only brings up Ollama for people who have it.
function setupEngineLine() {
  if (usesOllama()) {
    return `<p>wisperChad will use Ollama and the models in it.
      <a data-engine="builtin">Use the built-in engine instead</a></p>`;
  }
  const count = state.ollama.installed.length;
  const models = count ? `, with ${count} ${count === 1 ? "model" : "models"} in it` : "";
  const ollama = state.ollama.running && !writingInUse()
    ? ` We also found Ollama on this PC${models}. Would you rather use that? <a data-engine="ollama">Use Ollama</a>`
    : "";
  return `<p>Everything runs inside wisperChad, so there's nothing else to install.${ollama}</p>`;
}

async function closeSetup() {
  state.settings = { ...state.settings, setup_done: true };
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 6000));
  $("#setup").classList.add("hidden");
  renderHome();
  if (!state.settings.stt_model) showView("models");
}

function goToStep(step) {
  Object.assign(setup, { step, status: "", statusClass: "", passed: false, trying: false, locked: false, tipShown: false });
  renderSetup();
}

function pickPreset(preset) {
  Object.assign(setup, { speech: preset.speech_model, writing: preset.writing_model, presetName: preset.name });
}

async function showPresets() {
  setup.presets = state.presets = await invoke("get_presets", { languages: ["en"] });
  // Switching presets starts on the one in use, not the one we'd recommend.
  const current = state.settings.setup_done ? presetInUse() : null;
  pickPreset(current ?? setup.presets.find((p) => p.recommended));
  setup.tab = "presets";
  goToStep("preset");
}

function setupProgressRows(models) {
  return models
    .map((m) => `<div class="model" data-id="${esc(m.id)}">
        <div class="model-name">${esc(m.name)}</div>
        <div class="progress-slot">${progressHtml(m.id)}</div>
      </div>`)
    .join("");
}

// Lets someone running setup again leave without changing anything.
const setupCancel = () => (state.settings.setup_done ? `<a data-setup="later">Cancel</a>` : "");

function computerSpecs() {
  const hw = state.hardware;
  const gpu = bestGpu();
  return `<div class="setup-specs">
      <div><span class="k">Memory</span>${hw.ram_gb.toFixed(0)} GB</div>
      <div><span class="k">Graphics card</span>${gpu ? `${esc(gpu.name)} (${gpu.vram_gb.toFixed(0)} GB)` : "None found"}</div>
      <div><span class="k">Processor</span>${esc(hw.cpu)}</div>
    </div>`;
}

const downloadMb = (models) => models.filter(Boolean).reduce((sum, m) => sum + m.requirements.disk_mb, 0);

function presetCard(preset) {
  const speech = state.catalog.stt.find((m) => m.id === preset.speech_model);
  const writing = state.catalog.llm.find((m) => m.id === preset.writing_model);
  const picked = preset.speech_model === setup.speech && preset.writing_model === setup.writing;
  const left = [speech.installed ? null : speech, writing && !llmInstalled(writing) ? writing : null];
  const size = downloadMb(left);
  const inUse = state.settings.setup_done && presetInUse()?.id === preset.id;
  return `<button class="setup-preset ${picked ? "picked" : ""}" data-preset="${esc(preset.id)}">
      <div class="model-name">${esc(preset.name)}
        ${inUse ? `<span class="badge inuse">In use</span>` : ""}
        ${preset.recommended ? `<span class="badge rec">Recommended for your computer</span>` : ""}
        ${preset.fits ? "" : `<span class="badge warn">May run slowly on your computer</span>`}
      </div>
      <div class="model-desc">${esc(preset.summary)}</div>
      <div class="meta">
        <span>Listens with ${esc(speech.name)}</span>
        <span>${writing ? `Writes with ${esc(writing.name)}` : "No replies"}</span>
        <span>${size ? `${fmtMB(size)} download` : "Already installed"}</span>
      </div>
    </button>`;
}

function customRow(kind, model, picked) {
  const verdict = model.requirements.unknown ? null : fit(model.requirements, kind);
  return `<label class="setup-model">
      <input type="radio" name="${kind}" value="${esc(model.id)}" ${model.id === picked ? "checked" : ""} />
      <span>
        <span class="model-name">${esc(model.name)}
          ${verdict ? `<span class="badge ${verdict.vclass}">${verdict.verdict}</span>` : ""}
        </span>
        <span class="model-desc">${esc(model.description)}</span>
      </span>
    </label>`;
}

function customLists() {
  return `<h3>Listens with</h3>
    ${state.catalog.stt.map((m) => customRow("stt", m, setup.speech)).join("")}
    <h3>Writes with</h3>
    <label class="setup-model">
      <input type="radio" name="llm" value="" ${setup.writing ? "" : "checked"} />
      <span>
        <span class="model-name">None (transcription only)</span>
        <span class="model-desc">Types what you say, with numbers as digits and stutters taken out, but no other clean-up and no replies.</span>
      </span>
    </label>
    ${writingModels().map((m) => customRow("llm", m, setup.writing)).join("")}`;
}

function setupStatus() {
  return `<div class="setup-status ${setup.statusClass}">${esc(setup.status)}</div>`;
}

// Picks the hotkey a test uses, before trying it.
function setupHotkey(field, what) {
  return `
    <h3>Your ${what} hotkey</h3>
    <div class="field">
      <input class="hotkey-input" data-field="${field}" readonly value="${esc(keyNames(state.settings[field]))}" />
    </div>
    <p class="muted small">Click the box, then press the keys you want together and let go, like
      ${esc(DEFAULT_HOTKEYS[field])}. Keep it if it suits you.</p>`;
}

const holdStyle = () => (state.settings.hotkey_style || "hold") === "hold";
const setupHotkeyPill = (field) =>
  `<span class="hotkey" data-hotkey="${field === "dictate_hotkey" ? "dictate" : "reply"}">${esc(keyNames(state.settings[field]))}</span>`;

// After the first dictation, a pop-up says the hotkey can be double-tapped too.
function showDoubleTapTip() {
  const tip = document.createElement("div");
  tip.id = "setup-tip";
  tip.className = "setup-popup";
  tip.setAttribute("role", "dialog");
  tip.innerHTML = `
    <h3>You can also double-tap</h3>
    <p>Double-tap ${setupHotkeyPill("dictate_hotkey")} and let go. It keeps listening without you holding it, until you
      press it once more.</p>
    <button class="btn" data-setup="close-tip">Close</button>`;
  $("#setup-body").append(tip);
  $("#setup-box")?.focus();
}

// How to use a hotkey, in the style picked in Settings.
function setupHowToHold(field) {
  const hotkey = setupHotkeyPill(field);
  return holdStyle()
    ? `<p class="setup-tip">Press and hold ${hotkey} to listen, and let go when you're done. Or double-tap it to
        keep listening without holding it, and press it once more when you're done.</p>`
    : `<p class="setup-tip">Press ${hotkey} to start listening, and press it again when you're done.</p>`;
}

function setupButtons(next) {
  return `<div class="setup-actions">
      <button class="btn primary" data-setup="next">${next}</button>
    </div>`;
}

const SETUP_VIEWS = {
  preset() {
    const size = downloadMb([speechToInstall(), writingToInstall()]);
    const tab = (id, label) => `<button class="${setup.tab === id ? "on" : ""}" data-tab="${id}">${label}</button>`;
    const label = size ? `Install · ${fmtMB(size)}` : setup.presetsOnly ? "Use this preset" : "Start the tests";
    // From the Models page, the models already in use have nothing to switch to.
    const s = state.settings;
    const applied = setup.presetsOnly && !size && setup.speech === s.stt_model && (setup.writing ?? null) === (s.llm_model ?? null);
    const begin = applied
      ? `<button class="btn primary" disabled>Currently applied</button>`
      : `<button class="btn primary" data-setup="begin">${label}</button>`;
    return `
      <h1>${setup.presetsOnly ? "Choose a preset" : "Welcome to wisperChad"}</h1>
      <p class="lead">Here's your computer, and what we recommend for it.</p>
      ${computerSpecs()}
      <div class="setup-tabs">${tab("presets", "Presets")}${tab("custom", "Customize")}</div>
      ${setup.tab === "presets" ? setup.presets.map(presetCard).join("") : customLists()}
      <p class="muted small">Everything runs on your computer, so what you say stays on it. You can change this any time on the Models page.</p>
      ${setup.writing ? "" : `<p>Without a writing model, wisperChad can't write replies. It just types what you say. You can add one on the Models page later if your computer can run it.</p>`}
      ${setupEngineLine()}
      <div class="setup-actions">
        ${begin}
        ${setupCancel()}
      </div>`;
  },
  install() {
    return `
      <h1>Installing</h1>
      <p class="lead">You can keep using your PC while this runs.</p>
      <div class="model-list">${setupProgressRows([speechToInstall(), writingToInstall()].filter(Boolean))}</div>
      ${setup.error ? `
        <p class="fail">${esc(setup.error)}</p>
        <div class="setup-actions">
          <button class="btn primary" data-setup="begin">Try again</button>
          <a data-setup="later">Skip setup for now</a>
        </div>` : ""}`;
  },
  dictate() {
    return `
      <h1>Try dictating</h1>
      <p class="lead">wisperChad types what you say, wherever your cursor is.</p>
      ${setupHotkey("dictate_hotkey", "dictation")}
      <h3>Try it out</h3>
      <ol class="setup-how">
        <li>Click the box below.</li>
        ${holdStyle()
          ? `<li>Hold ${setupHotkeyPill("dictate_hotkey")} and say something, like "testing, um, one two three".</li>
             <li>Let go. Your words appear in the box.</li>`
          : `<li>Press ${setupHotkeyPill("dictate_hotkey")}, say something, like "testing, um, one two three", and press
               it again. Your words appear in the box.</li>`}
      </ol>
      <textarea id="setup-box" rows="3" placeholder="Your words appear here"></textarea>
      ${setupStatus()}
      ${setupButtons("Next")}`;
  },
  reply() {
    const reads = state.settings.reply_source === "screen";
    return `
      <h1>Try writing a reply</h1>
      <p class="lead">Say what you mean, however rough. wisperChad writes it properly.</p>
      ${setupHotkey("reply_hotkey", "reply")}
      <h3>Try it out</h3>
      ${setupHowToHold("reply_hotkey")}
      <div class="setup-email">
        <div class="muted small">From Sarah Lim</div>
        <b>Team lunch on Friday?</b>
        <p>Hi! We're planning a team lunch this Friday at 12:30. Can you make it? Also, do you have any food allergies?</p>
        <p>Thanks,<br>Sarah</p>
      </div>
      <ol class="setup-how">
        ${reads ? "" : `<li>Select the email above and press <kbd>${SHORTCUT_KEY}</kbd>+<kbd>C</kbd>.</li>`}
        <li>Click the reply box.</li>
        <li>${holdStyle() ? "Hold" : "Press"} ${setupHotkeyPill("reply_hotkey")}${holdStyle() ? ", or double-tap it," : ""} and say your answer, like "yeah I can come, no allergies".</li>
        <li>${holdStyle() ? "Let go, or press it once more if you double-tapped." : "Press it again."} ${reads ? "wisperChad reads the email and" : "wisperChad"} writes the reply. The first one can take a little longer.</li>
      </ol>
      <textarea id="setup-box" rows="6" placeholder="Reply to Sarah"></textarea>
      ${setupStatus()}
      ${setupButtons("Next")}`;
  },
  allow() {
    const rows = setup.permissions.map((p) => `
      <div class="setup-permission">
        <div>
          <b>${esc(p.name)}</b>
          <div class="muted small">${esc(p.why)}</div>
        </div>
        ${p.allowed === true
          ? `<span class="pass">✓ Allowed</span>`
          : `<button class="btn" data-allow="${esc(p.id)}">Allow</button>`}
      </div>`).join("");
    return `
      <h1>Allow wisperChad</h1>
      <p class="lead">It needs these to hear you and type for you. Click Allow on each one: your computer either asks
        you, or opens the setting to switch on.</p>
      <div class="setup-permissions">${rows}</div>
      <p class="muted small">This page notices by itself when you've switched one on.</p>
      <div class="setup-actions"><button class="btn primary" data-setup="next">Next</button></div>`;
  },
  name() {
    return `
      <h1>What's your name?</h1>
      <p class="lead">wisperChad signs your emails with it, and it helps identify who you are when replying. You'll
        see it on the test reply in a minute.</p>
      <div class="field">
        <input id="setup-name" value="${esc(state.settings.user_name || "")}" placeholder="As it shows on LinkedIn, like Alex Tan" />
      </div>
      <p class="muted small">So when you were the last to write and haven't had an answer, a reply follows up with
        them instead of answering you. It stays on this PC, and you can change it in Settings.</p>
      <div class="setup-actions">
        <button class="btn primary" data-setup="save-name">Next</button>
        <a data-setup="skip-name">Skip</a>
      </div>`;
  },
  done() {
    return `
      <h1>You're all set</h1>
      <p class="lead">${setup.skipped
        ? "You skipped a test. To try it again, go to Settings and click Run setup again."
        : "Everything works, and you won't need to do this again."}</p>
      <div class="grid2">
        <div class="card">
          <h3>Dictate</h3>
          <div class="hotkey" data-hotkey="dictate">${esc(keyNames(state.settings.dictate_hotkey))}</div>
          <div class="muted">Types what you say, in any app.</div>
        </div>
        <div class="card">
          <h3>Reply</h3>
          ${setup.writing
            ? `<div class="hotkey" data-hotkey="reply">${esc(keyNames(state.settings.reply_hotkey))}</div>
               <div class="muted">Writes a reply to the message that's open.</div>`
            : `<div class="muted">Off, because replies need a writing model. Add one on the Models page whenever you want them.</div>`}
        </div>
      </div>
      <p class="muted">wisperChad keeps running in the system tray when you close this window. It never sends anything for you: you read the draft and press Send yourself.</p>
      <p class="muted">To change your models later, go to Settings and click Run setup again.</p>
      <div class="setup-actions"><button class="btn primary" data-setup="finish">Start using wisperChad</button></div>`;
  },
};

// The permissions page shows only when something isn't allowed yet, and keeps checking while it's up.
async function goToTests() {
  setup.permissions = (await invoke("get_permissions").catch(() => null)) || [];
  goToStep(setup.permissions.some((p) => p.allowed !== true) ? "allow" : "dictate");
}

let permissionCheck = null;
function watchPermissions() {
  clearInterval(permissionCheck);
  if (setup.step !== "allow") return;
  permissionCheck = setInterval(async () => {
    if (setup.step !== "allow" || $("#setup").classList.contains("hidden")) return clearInterval(permissionCheck);
    const now = (await invoke("get_permissions").catch(() => null)) || setup.permissions;
    if (JSON.stringify(now) === JSON.stringify(setup.permissions)) return;
    setup.permissions = now;
    renderSetup();
  }, 2000);
}

function renderSetup() {
  const index = SETUP_STEPS.indexOf(setup.step);
  $("#setup-dots").innerHTML = SETUP_STEPS.map((_, i) => `<span class="${i <= index ? "on" : ""}"></span>`).join("");
  $("#setup-dots").classList.toggle("hidden", !!setup.presetsOnly);
  $("#setup-body").innerHTML = SETUP_VIEWS[setup.step]();
  ($("#setup-box") ?? $("#setup-name"))?.focus();
  watchPermissions();
}

// Only the status line and button change during a test, so what's in the box isn't redrawn away.
function showTestStatus(message, statusClass = "") {
  Object.assign(setup, { status: message, statusClass });
  const line = $("#setup-body .setup-status");
  if (!line) return;
  line.textContent = message;
  line.className = `setup-status ${statusClass}`;
}

async function saveSetupChoices() {
  state.settings = {
    ...state.settings,
    stt_model: setup.speech,
    llm_model: setup.writing,
    // A model that can read the open window saves copying the message first.
    reply_source: pickedWriting()?.files?.length > 1 || !setup.writing ? "screen" : "clipboard",
  };
  // The choices are saved even when a hotkey can't be set, which shouldn't stop setup. It says why.
  await invoke("save_settings", { settings: state.settings }).catch((e) => toast(String(e), 8000));
  fillSettings();
  await refreshAll();
}

async function installForSetup() {
  const speech = speechToInstall();
  const writing = writingToInstall();
  Object.assign(setup, { step: "install", error: "" });
  renderSetup();
  try {
    if (speech) await install("stt", speech.id);
    if (writing) await install("llm", writing.id);
    await refreshAll();
    await saveSetupChoices();
    await afterInstall();
  } catch (err) {
    setup.error = String(err);
    renderSetup();
  }
}

const SETUP_TEST_MODES = { dictate: "dictate", reply: "reply" };

// The name is for replies, so with no writing model there's nothing to ask it for. Asked before the
// tests, it's already on the test reply.
// Switching presets from the Models page ends here.
async function afterInstall() {
  if (setup.presetsOnly) {
    await closeSetup();
    return toast(setup.presetName ? `Now using ${setup.presetName}` : "Models switched");
  }
  return setup.writing ? goToStep("name") : goToTests();
}

const WORKING_MESSAGES = {
  listening: () => (setup.locked || !holdStyle() ? "Listening… press the keys once more when you're done." : "Listening… let go when you're done."),
  transcribing: () => "Turning your voice into text…",
  writing: () => "Writing…",
};

// A double-press shows up just before the listening bar, so the test knows which way was tried.
listen("hotkey-locked", ({ payload }) => {
  if (payload === SETUP_TEST_MODES[setup.step]) setup.locked = true;
});

listen("pill-status", ({ payload }) => {
  const mode = SETUP_TEST_MODES[setup.step];
  if (!mode || $("#setup").classList.contains("hidden")) return;
  if (WORKING_MESSAGES[payload.state] && payload.mode === mode) {
    setup.trying = true;
    showTestStatus(WORKING_MESSAGES[payload.state]());
  } else if (payload.state === "error") {
    Object.assign(setup, { trying: false, locked: false });
    showTestStatus(payload.message, "fail");
  } else if (payload.state === "hidden" && setup.trying) {
    Object.assign(setup, { trying: false, locked: false });
    showTestStatus("We didn't catch anything. Check your microphone is on, then try again.", "warn");
  }
});

// Says when a test worked. The first dictation also brings up the double-tap pop-up.
listen("history", ({ payload }) => {
  if (payload.mode !== SETUP_TEST_MODES[setup.step]) return;
  Object.assign(setup, { passed: true, trying: false, locked: false });
  showTestStatus(`✓ It works! We heard: "${payload.heard}"`, "pass");
  if (setup.step === "dictate" && holdStyle() && !setup.tipShown) {
    setup.tipShown = true;
    showDoubleTapTip();
  }
});

$("#setup-body").addEventListener("click", async (e) => {
  const preset = e.target.closest("[data-preset]");
  if (preset) {
    pickPreset(setup.presets.find((p) => p.id === preset.dataset.preset));
    return renderSetup();
  }
  const tab = e.target.closest("[data-tab]");
  if (tab) {
    setup.tab = tab.dataset.tab;
    return renderSetup();
  }
  const engine = e.target.closest("[data-engine]")?.dataset.engine;
  if (engine) {
    await setEngine(engine);
    if (engine === "ollama") setup.tab = "custom"; // That's where the models already in it are listed.
    return renderSetup();
  }
  const allow = e.target.closest("[data-allow]")?.dataset.allow;
  if (allow) return invoke("request_permission", { id: allow }).catch((err) => toast(String(err), 6000));
  const action = e.target.closest("[data-setup]")?.dataset.setup;
  if (action === "begin") {
    if (speechToInstall() || writingToInstall()) return installForSetup();
    await saveSetupChoices();
    return afterInstall();
  }
  if (action === "next") {
    // Moving on is always allowed; setup's last page then says how to try a test again.
    if (SETUP_TEST_MODES[setup.step] && !setup.passed) setup.skipped = true;
    const next = SETUP_STEPS[SETUP_STEPS.indexOf(setup.step) + 1];
    return goToStep(next === "reply" && !setup.writing ? "done" : next);
  }
  if (action === "save-name") {
    state.settings = { ...state.settings, user_name: $("#setup-name").value.trim() };
    await invoke("save_settings", { settings: state.settings }).catch((err) => toast(String(err), 6000));
    fillSettings();
    return goToTests();
  }
  if (action === "skip-name") return goToTests();
  if (action === "close-tip") {
    $("#setup-tip")?.remove();
    return $("#setup-box")?.focus();
  }
  if (action === "later" || action === "finish") await closeSetup();
});

$("#setup-body").addEventListener("keydown", (e) => {
  if (e.target.id === "setup-name" && e.key === "Enter") $("#setup-body [data-setup='save-name']").click();
});

// Only the model lists redraw the page. The test boxes change too when they lose focus, and redrawing
// then would empty them and swallow the click on Next.
$("#setup-body").addEventListener("change", (e) => {
  if (e.target.name === "stt") setup.speech = e.target.value;
  else if (e.target.name === "llm") setup.writing = e.target.value || null;
  else return;
  setup.presetName = ""; // Picked by hand, so it's no longer that preset.
  renderSetup();
});

$("#run-setup").addEventListener("click", () => openSetup());
$("#open-presets").addEventListener("click", () => openSetup(true));
