// Adapt fixi's requests to the server's existing form and navigation semantics.
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

document.addEventListener("fx:config", (event) => {
  const { cfg } = event.detail;
  cfg.source = event.target;

  if (reducedMotion.matches) {
    cfg.transition = false;
  }

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

document.addEventListener("fx:after", (event) => {
  const { response } = event.detail.cfg;

  if (response.redirected) {
    event.preventDefault();
    window.location.assign(response.url);
    return;
  }

  if (!response.ok) {
    event.preventDefault();
  }
});

document.addEventListener("fx:swapped", (event) => {
  const { source, focusAfterSwap } = event.detail.cfg;
  const targetSelector = source.getAttribute("fx-target");
  const replacement = targetSelector && document.querySelector(targetSelector);
  const focusTarget =
    replacement?.querySelector('[aria-invalid="true"], [role="status"]') ?? focusAfterSwap;

  if (focusTarget?.isConnected) {
    focusTarget.focus();
  }
});
