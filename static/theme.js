// The theme chosen on the settings page, kept in this browser, applied
// before the page is drawn so it never shows the other one first. With
// none chosen, or System, the page follows the system's.
const THEME = "theme";

const apply = (theme) => {
  document.documentElement.style.colorScheme =
    theme === "light" || theme === "dark" ? theme : "";
};

apply(localStorage.getItem(THEME));

// The settings page's choice, marked as it stands and kept as it changes.
document.addEventListener("DOMContentLoaded", () => {
  const chosen = localStorage.getItem(THEME) ?? "system";
  for (const input of document.querySelectorAll('input[name="theme"]')) {
    input.checked = input.value === chosen;
  }
});
document.addEventListener("change", (event) => {
  if (!event.target.matches?.('input[name="theme"]')) return;
  const theme = event.target.value;
  if (theme === "system") localStorage.removeItem(THEME);
  else localStorage.setItem(THEME, theme);
  apply(theme);
});
