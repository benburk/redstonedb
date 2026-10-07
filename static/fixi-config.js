// Adapt fixi's requests to the server's existing form and navigation semantics.
const notice = document.getElementById("notice");

function showNotice(message) {
  notice.textContent = "";
  notice.textContent = message;
  clearTimeout(showNotice.timer);
  showNotice.timer = setTimeout(() => {
    notice.textContent = "";
  }, 6000);
}

document.addEventListener("fx:config", (event) => {
  const { cfg } = event.detail;
  cfg.source = event.target;
  cfg.transition = null;
  // The swap may replace the focused control; fx:swapped looks for its id.
  cfg.focusedId = document.activeElement?.id;

  if (cfg.body instanceof FormData) {
    cfg.body = new URLSearchParams(cfg.body);
  }

  const message = event.target.getAttribute("ext-fx-confirm");
  if (message) {
    cfg.confirm = () => window.confirm(message);
  }

  const focusFallback = event.target.getAttribute("data-focus-after-remove");
  if (focusFallback) {
    cfg.focusAfterSwap =
      cfg.target.nextElementSibling?.querySelector("[data-focus-after-remove]") ??
      cfg.target.previousElementSibling?.querySelector("[data-focus-after-remove]") ??
      document.querySelector(focusFallback);
  }
});

document.addEventListener("fx:before", (event) => {
  const { cfg } = event.detail;
  cfg.busy = cfg.trigger?.submitter ?? event.target;
  cfg.busy.setAttribute("aria-busy", "true");
});

document.addEventListener("fx:finally", (event) => {
  event.detail.cfg.busy?.removeAttribute("aria-busy");
});

document.addEventListener("fx:after", (event) => {
  const { response, text } = event.detail.cfg;

  if (response.redirected) {
    event.preventDefault();
    window.location.assign(response.url);
    return;
  }

  if (response.status === 204) {
    event.preventDefault();
    return;
  }

  if (!response.ok || text.trim() === "") {
    event.preventDefault();
    showNotice("The server answered with an error. Nothing was changed.");
  }
});

document.addEventListener("fx:error", (event) => {
  if (event.detail.error?.name === "AbortError") return;
  showNotice("Couldn't reach the server.");
});

document.addEventListener("fx:swapped", (event) => {
  const { source, focusAfterSwap, focusedId } = event.detail.cfg;
  const targetSelector = source.getAttribute("fx-target");
  const replacement = targetSelector && document.querySelector(targetSelector);
  const focusTarget =
    replacement?.querySelector('[aria-invalid="true"], [role="status"]') ??
    (focusedId && document.getElementById(focusedId)) ??
    focusAfterSwap ??
    replacement?.querySelector("[tabindex='-1']");

  if (focusTarget?.isConnected) {
    focusTarget.focus();
  }
});
