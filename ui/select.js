// Windows draws a native dropdown's list in its own window, so it can hang outside the app. These
// stay inside it. The real <select> stays hidden underneath, holding the value and firing "change".
const customSelects = [];

const selectLabel = (select) => select.selectedOptions[0]?.textContent ?? "";

function closeSelects() {
  document.querySelectorAll(".select-list").forEach((list) => list.remove());
  document.querySelectorAll(".select-button").forEach((button) => button.setAttribute("aria-expanded", "false"));
}

// Opens below the box, or above it if that leaves more room, and never past the window's edge.
function placeSelectList(list, button) {
  const gap = 6;
  const edge = 12;
  const box = button.getBoundingClientRect();
  const below = window.innerHeight - box.bottom - gap - edge;
  const above = box.top - gap - edge;
  const upward = list.scrollHeight > below && above > below;
  const height = Math.min(list.scrollHeight, upward ? above : below);
  list.style.left = `${box.left}px`;
  list.style.width = `${box.width}px`;
  list.style.maxHeight = `${height}px`;
  list.style.top = `${upward ? box.top - gap - height : box.bottom + gap}px`;
}

function openSelect(select, button) {
  closeSelects();
  const list = document.createElement("div");
  list.className = "select-list";
  list.setAttribute("role", "listbox");
  for (const option of select.options) {
    const item = document.createElement("button");
    item.type = "button";
    item.textContent = option.textContent;
    item.setAttribute("role", "option");
    item.classList.toggle("picked", option.selected);
    item.addEventListener("click", () => {
      select.value = option.value;
      select.dispatchEvent(new Event("change"));
      button.textContent = selectLabel(select);
      closeSelects();
      button.focus();
    });
    list.append(item);
  }
  list.addEventListener("keydown", (e) => {
    const step = { ArrowDown: "nextElementSibling", ArrowUp: "previousElementSibling" }[e.key];
    if (step) {
      e.preventDefault();
      document.activeElement[step]?.focus();
    } else if (e.key === "Escape") {
      closeSelects();
      button.focus();
    }
  });
  document.body.append(list);
  placeSelectList(list, button);
  button.setAttribute("aria-expanded", "true");
  (list.querySelector(".picked") ?? list.firstElementChild).focus();
}

function refreshSelects() {
  customSelects.forEach(({ select, button }) => (button.textContent = selectLabel(select)));
}

document.querySelectorAll("select").forEach((select) => {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "select-button";
  button.setAttribute("aria-haspopup", "listbox");
  button.setAttribute("aria-expanded", "false");
  button.addEventListener("click", () => {
    if (button.getAttribute("aria-expanded") === "true") return closeSelects();
    openSelect(select, button);
  });
  select.classList.add("hidden");
  select.after(button);
  customSelects.push({ select, button });
});
refreshSelects();

document.addEventListener("click", (e) => {
  if (!e.target.closest(".select-list, .select-button")) closeSelects();
});
window.addEventListener("resize", closeSelects);
document.querySelector("main").addEventListener("scroll", closeSelects);
