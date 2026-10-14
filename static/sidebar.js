// The category sidebar. On a wide screen it stays in view as the page
// scrolls and scrolls itself, so its height is fitted to what the window
// shows below its top, which is lower until the page scrolls past the
// header. A category opens a new page, so where the sidebar was scrolled
// to is kept for it, and the new page starts there.
const KEPT = "sidebar-scroll";
const MARGIN = 16;

for (const sidebar of document.querySelectorAll("[data-sidebar]")) {
  const fit = () => {
    if (getComputedStyle(sidebar).position !== "sticky") {
      sidebar.style.maxHeight = "";
      return;
    }
    const top = Math.max(sidebar.getBoundingClientRect().top, MARGIN);
    sidebar.style.maxHeight = `${window.innerHeight - top - MARGIN}px`;
  };
  fit();
  addEventListener("scroll", fit, { passive: true });
  addEventListener("resize", fit);

  const kept = sessionStorage.getItem(KEPT);
  sessionStorage.removeItem(KEPT);
  if (kept !== null) sidebar.scrollTop = Number(kept);
  sidebar.addEventListener("click", (event) => {
    if (event.target.closest("a")) {
      sessionStorage.setItem(KEPT, String(sidebar.scrollTop));
    }
  });
}
