// Picks the white or black look before the page draws, so it never flashes the other one.
const THEME_KEY = "wisperchad.theme";
const DEFAULT_THEME = "light";
const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

function savedTheme() {
  try {
    return localStorage.getItem(THEME_KEY) || DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

function applyTheme(theme) {
  const dark = theme === "dark" || (theme === "system" && systemDark.matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function setTheme(theme) {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Only costs the choice on the next launch.
  }
  applyTheme(theme);
}

applyTheme(savedTheme());
systemDark.addEventListener("change", () => applyTheme(savedTheme()));
