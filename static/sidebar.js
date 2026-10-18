// The sidebar. On a wide screen it stays in view and scrolls itself, and
// following one of its links opens a new page, so where it was scrolled to
// is kept for it, and the new page's starts there.
const KEPT = "sidebar-scroll";

const sidebar = document.querySelector("[data-sidebar]");
if (sidebar) {
  const kept = sessionStorage.getItem(KEPT);
  sessionStorage.removeItem(KEPT);
  if (kept !== null) sidebar.scrollTop = Number(kept);
  sidebar.addEventListener("click", (event) => {
    if (event.target.closest("a")) {
      sessionStorage.setItem(KEPT, String(sidebar.scrollTop));
    }
  });
}
