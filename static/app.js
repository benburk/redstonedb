// How the site takes fixi's answers, for every page. fixi swaps whatever
// fetch() resolves with, errors and empty bodies too, so this decides what
// may replace part of a page, says so when something went wrong, and keeps
// the reader's place across a swap.
//
// Conventions the pages follow:
// - `data-confirm` on a trigger asks before sending.
// - `data-form="id"` sends that form's fields from an element outside it.
// - A live panel has `fx-trigger="redraw"` and a stable id; an answer's
//   `FX-Redraw` header names panels to redraw, space-separated.
// - `fx-trigger="load"` sends once the element is on the page.
// - `fx-trigger="poll"` with `data-every="ms"` sends every so often while
//   the element is on the page.
// - An answer's `FX-Notice` header, percent-encoded, is said in the notice.
// - An answer's `FX-Replace-Url` header becomes the page's address.
// - A `<dialog data-show>` in an answer opens as a modal once it is in.
// - A dialog's form with `data-wait` keeps the dialog open while it sends,
//   so it shows the work under way, and closes it once answered.
// - A form with `data-live` sends as its fields change, typing once it
//   pauses, and a new send replaces one under way, as a search does.

const notice = document.getElementById("notice");

// Says `text` in the page's one status region, outside every swap.
const showNotice = (text) => {
  notice.textContent = "";
  requestAnimationFrame(() => {
    notice.textContent = text;
  });
};

const LIVE = '[fx-trigger="redraw"]';

const redraw = (panel) => panel?.dispatchEvent(new Event("redraw"));

// A live panel takes only an answer that is itself, so a stray page or
// error never lands in it, and keeps its scroll and open details.
const swapPanel = ({ target, text, response }) => {
  if (!target.isConnected || !response.ok) return;
  const next = document.createElement("template");
  next.innerHTML = text;
  const panel = next.content.firstElementChild;
  if (panel?.id !== target.id) return;
  const { scrollTop } = target;
  const open = new Set(
    [...target.querySelectorAll("details[open][id]")].map((d) => d.id),
  );
  target.replaceWith(panel);
  panel.scrollTop = scrollTop;
  for (const id of open) {
    panel.querySelector(`details#${CSS.escape(id)}`)?.setAttribute("open", "");
  }
};

document.addEventListener("fx:config", (event) => {
  const { cfg, requests } = event.detail;
  const element = event.target;
  cfg.transition = null;
  const trigger = cfg.trigger.submitter ?? element;
  const question = trigger.dataset?.confirm;
  if (question) cfg.confirm = () => confirm(question);
  const named = element.dataset.form;
  if (named) cfg.body = new FormData(document.getElementById(named));
  // Fields go URL-encoded, without any file, unless a form sends files.
  const sendsFiles =
    element.matches("form") && element.enctype === "multipart/form-data";
  if (cfg.body instanceof FormData && !sendsFiles) {
    cfg.body = new URLSearchParams(
      [...cfg.body].filter(([, value]) => typeof value === "string"),
    );
  }
  // Only the latest search's answer matters.
  if (element.matches("form[data-live]")) {
    for (const request of requests) request.abort();
    cfg.drop = 0;
  }
  if (element.matches(LIVE)) {
    // A redraw stands in for any request still under way in the panel,
    // its own or one inside it, which the server then stops.
    for (const request of requests) request.abort();
    for (const inner of element.querySelectorAll("[fx-action]")) {
      for (const request of inner.__fixi?.requests ?? []) request.abort();
    }
    cfg.drop = 0;
    cfg.swap = swapPanel;
  }
  // A dialog's form closes it as it sends, so the swap never lands under
  // an open modal, unless it waits for the answer.
  if (element.closest("dialog[open]") && !element.matches("[data-wait]")) {
    element.closest("dialog").close();
  }
});

// The control that sent a request shows it is busy; a live panel is its
// own trigger and target, so it does not.
const sender = (event) =>
  event.detail.cfg.trigger.submitter ?? event.target;

// Where focus was, to put it back once the swap replaces it.
let focused = null;

document.addEventListener("fx:before", (event) => {
  if (!event.target.matches(LIVE)) {
    sender(event).setAttribute("aria-busy", "true");
  }
  const active = document.activeElement;
  focused = active?.id
    ? {
      id: active.id,
      start: active.selectionStart,
      end: active.selectionEnd,
    }
    : null;
});

document.addEventListener("fx:finally", (event) => {
  sender(event).removeAttribute("aria-busy");
  if (event.target.matches("[data-wait]")) event.target.closest("dialog")?.close();
});

document.addEventListener("fx:after", (event) => {
  const { cfg } = event.detail;
  const { response, text } = cfg;
  if (response.redirected) {
    event.preventDefault();
    location.assign(response.url);
    return;
  }
  // The address that shows what the answer brings, such as a search's
  // results, so Back and reloading keep them.
  const at = response.headers.get("FX-Replace-Url");
  if (at && response.ok) history.replaceState(history.state, "", at);
  const said = response.headers.get("FX-Notice");
  if (said) showNotice(decodeURIComponent(said));
  const changed = response.headers.get("FX-Redraw");
  for (const id of changed?.split(" ") ?? []) {
    // Redrawn once this swap is in.
    queueMicrotask(() => redraw(document.getElementById(id)));
  }
  // Nothing to change, or nowhere left to change it.
  if (response.status === 204 || !cfg.target.isConnected) {
    event.preventDefault();
    return;
  }
  if (response.ok && text.trim() !== "") return;
  event.preventDefault();
  showNotice("The server answered with an error. Try again.");
});

document.addEventListener("fx:error", (event) => {
  if (event.detail.error?.name === "AbortError") return;
  showNotice("Couldn't reach the server. Try again.");
});

// A live form sends once typing pauses, or at once for a menu. Its
// fields may sit outside it, naming it in their `form` attribute.
const PAUSE = 250;
let pause;
document.addEventListener("input", (event) => {
  const field = event.target;
  const form = field.form;
  if (!form?.matches("[data-live]") || event.isComposing) return;
  clearTimeout(pause);
  const wait = field.matches("select") ? 0 : PAUSE;
  pause = setTimeout(() => form.requestSubmit(), wait);
});
// Enter sends at once, so the pause's send is dropped.
document.addEventListener("submit", () => clearTimeout(pause));

document.addEventListener("fx:swapped", () => {
  for (const dialog of document.querySelectorAll("dialog[data-show]:not([open])")) {
    dialog.removeAttribute("data-show");
    dialog.showModal();
  }
});

// A swap that took the focused element away puts focus back on its
// replacement, the element with the same id, where the caret was.
document.addEventListener("fx:swapped", () => {
  if (!focused || document.activeElement !== document.body) return;
  const again = document.getElementById(focused.id);
  if (!again) return;
  again.focus();
  if (focused.start != null && again.setSelectionRange) {
    again.setSelectionRange(focused.start, focused.end);
  }
});

// `fx:inited` does not bubble, so it is heard on the way down instead.
document.addEventListener(
  "fx:inited",
  (event) => {
    const element = event.target;
    const trigger = element.getAttribute("fx-trigger");
    if (trigger === "load") {
      queueMicrotask(() => element.dispatchEvent(new Event("load")));
    } else if (trigger === "poll") {
      const every = Number(element.dataset.every ?? 3000);
      const tick = () => {
        if (!element.isConnected) return;
        element.dispatchEvent(new Event("poll"));
        setTimeout(tick, every);
      };
      setTimeout(tick, every);
    }
  },
  true,
);

// A page come back to shows what changed meanwhile.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    document.querySelectorAll(LIVE).forEach(redraw);
  }
});
addEventListener("pageshow", (event) => {
  if (event.persisted) document.querySelectorAll(LIVE).forEach(redraw);
});

// A dialog's download form closes it once the download starts, since a
// download leaves the page where it is.
for (const dialog of document.querySelectorAll("dialog")) {
  dialog
    .querySelector('form[method="get"]')
    ?.addEventListener("submit", () => dialog.close());
}
