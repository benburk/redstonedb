// The category sidebar. On a wide screen it stays in view as the page
// scrolls and scrolls itself, so its height is fitted to what the window
// shows below its top, which is lower until the page scrolls past the
// header. A category opens a new page, so where the sidebar was scrolled
// to is kept for it, and the new page starts there. A search as it is
// typed redraws the sidebar, so it is looked up each time, and keeps its
// scroll across the redraw.
const KEPT = "sidebar-scroll";
const MARGIN = 16;

const sidebar = () => document.querySelector("[data-sidebar]");

const fit = () => {
  const element = sidebar();
  if (!element) return;
  if (getComputedStyle(element).position !== "sticky") {
    element.style.maxHeight = "";
    return;
  }
  const top = Math.max(element.getBoundingClientRect().top, MARGIN);
  element.style.maxHeight = `${window.innerHeight - top - MARGIN}px`;
};

fit();
addEventListener("scroll", fit, { passive: true });
addEventListener("resize", fit);

const kept = sessionStorage.getItem(KEPT);
sessionStorage.removeItem(KEPT);
if (kept !== null && sidebar()) sidebar().scrollTop = Number(kept);

document.addEventListener("click", (event) => {
  const element = sidebar();
  if (element?.contains(event.target) && event.target.closest("a")) {
    sessionStorage.setItem(KEPT, String(element.scrollTop));
  }
});

let scrolled = 0;
document.addEventListener("fx:before", () => {
  scrolled = sidebar()?.scrollTop ?? 0;
});
document.addEventListener("fx:swapped", () => {
  const element = sidebar();
  if (!element) return;
  element.scrollTop = scrolled;
  fit();
});
