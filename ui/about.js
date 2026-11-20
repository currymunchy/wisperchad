// The About page, and the small note that shows once after an update.
const VERSION_KEY = "wisperchad.version";
const WHATS_NEW = {
  version: "0.11.27",
  items: [
    "Saying “I don’t care” or “whatever” to an email gives a polite no thanks, or “either works” when they ask you to pick",
    "On a Mac, the listening bar sits just above the Dock, as it does above the taskbar on Windows",
  ],
};

function lastVersionSeen(version) {
  try {
    const seen = localStorage.getItem(VERSION_KEY);
    localStorage.setItem(VERSION_KEY, version);
    return seen;
  } catch {
    return version; // Without storage this can't tell, so it stays quiet rather than repeating.
  }
}

function showUpdateNote() {
  const note = document.createElement("div");
  note.className = "update-note";
  note.innerHTML = `<button aria-label="Close">×</button>
    <h3>New in ${esc(WHATS_NEW.version)}</h3>
    <ul>${WHATS_NEW.items.map((item) => `<li>${esc(item)}</li>`).join("")}</ul>`;
  note.querySelector("button").addEventListener("click", () => note.remove());
  document.body.append(note);
}

// A fresh install has nothing new to report, so setup not being done rules that out.
async function renderAbout() {
  const about = await invoke("get_about");
  $("#about-version").textContent = `Version ${about.version}`;
  $("#about-data").textContent = about.data_dir;
  $("#about-log").textContent = about.log_dir;
  $("#about-custom-replies").classList.toggle("hidden", !about.custom_reply_prompts);
  // From the Store, Windows Settings decides whether the app starts with Windows.
  $("#set-launch").closest(".field").classList.toggle("hidden", Boolean(about.store));
  $("#launch-store-note").classList.toggle("hidden", !about.store);
  const updated = lastVersionSeen(about.version) !== about.version;
  if (updated && state.settings.setup_done && about.version === WHATS_NEW.version) showUpdateNote();
}

document.querySelectorAll("[data-folder]").forEach((button) =>
  button.addEventListener("click", () =>
    invoke("open_folder", { which: button.dataset.folder }).catch((e) => toast(String(e), 6000))
  )
);

// Opens in the browser. The app only knows the pages it links to by name.
document.querySelectorAll("[data-link]").forEach((button) =>
  button.addEventListener("click", () =>
    invoke("open_link", { which: button.dataset.link }).catch((e) => toast(String(e), 6000))
  )
);

// Feedback goes by email through Web3Forms. Its free plan only takes messages sent from a web page,
// so this page sends it rather than the Rust side. The key can only send to the developer's inbox,
// so it's made to be shipped in an app. Without one the box stays hidden.
const FEEDBACK_KEY = "4fd3b01e-227f-4bad-8b9b-9ee9656b30ef";

async function sendFeedback() {
  const text = $("#feedback-text").value.trim();
  const email = $("#feedback-email").value.trim();
  const status = $("#feedback-status");
  if (!text) {
    status.textContent = "Write something first.";
    return;
  }
  const button = $("#feedback-send");
  button.disabled = true;
  status.textContent = "Sending…";
  const system = ON_MAC ? "Mac" : ON_LINUX ? "Linux" : "Windows";
  const about = await invoke("get_about").catch(() => ({}));
  const version = about.version || "unknown";
  const message = {
    access_key: FEEDBACK_KEY,
    subject: `wisperChad feedback (${version}, ${system})`,
    from_name: "wisperChad",
    message: text,
    version,
    system,
  };
  if (email) message.email = email;
  try {
    const response = await fetch("https://api.web3forms.com/submit", {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify(message),
    });
    const answer = await response.json().catch(() => ({}));
    if (!response.ok || !answer.success) throw new Error(answer.message || `The server said ${response.status}.`);
    $("#feedback-text").value = "";
    status.textContent = "Sent. Thank you!";
  } catch (e) {
    // fetch throws a TypeError when it can't connect at all.
    status.textContent =
      e instanceof TypeError
        ? "Couldn't reach the internet. Check your connection and try again."
        : `Couldn't send it: ${e.message}`;
  } finally {
    button.disabled = false;
  }
}

$(".about-feedback").classList.toggle("hidden", !FEEDBACK_KEY);
$("#feedback-send").addEventListener("click", sendFeedback);

// The full texts are long, so they only load when someone asks to see them.
function showLicenses() {
  showView("licenses");
  $(".nav[data-view='about']").classList.add("active");
  if (window.THIRD_PARTY_LICENSES) return;
  const script = document.createElement("script");
  script.src = "licenses.js";
  script.onload = () => ($("#licenses-text").textContent = THIRD_PARTY_LICENSES);
  document.body.append(script);
}

$("#show-licenses").addEventListener("click", showLicenses);
$("#licenses-back").addEventListener("click", () => showView("about"));
