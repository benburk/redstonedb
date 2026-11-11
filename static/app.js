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
// - An answer's `FX-Redirect` header loads that page instead, as when
//   what was polled for is done.
// - A `<dialog data-show>` in an answer opens as a modal once it is in.
// - A dialog's form with `data-wait` keeps the dialog open while it sends,
//   so it shows the work under way, and closes it once answered. One whose
//   answer lands in the dialog keeps it open.
// - A field with `data-caret-end` takes focus with the caret at its end,
//   as when its dialog opens on it.
// - A form with `data-live` sends as its fields change, typing once it
//   pauses, and a new send replaces one under way, as a search does.
// - A button with `data-disclose` shows or hides the elements its
//   `aria-controls` names, space-separated, its `aria-expanded` saying
//   which, as an output's bit rows.
// - A `<details data-menu>` is a menu: it closes when a choice in it is
//   made, or on a click outside it or Escape.
// - A form with `data-analyzing` gives way to its `[data-uploading]` steps
//   as soon as it sends, and `data-progress="url"` redraws an upload's
//   steps from that address until it answers `X-Analysis: done`, then
//   loads the page again.
// - A `<details>` whose id the address names opens, as a build's check its
//   stage's link goes to.

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

// Once the page is going elsewhere, nothing polls, so a later answer can't
// start the going over again before the next page is in. Going to this
// page itself, as an analysis's page does once it is done, draws it again
// in its own place, not as another step back.
let leaving = false;
const leave = (url) => {
  if (leaving) return;
  leaving = true;
  if (new URL(url, location.href).href === location.href) {
    location.replace(url);
  } else {
    location.assign(url);
  }
};

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
  // A field that sends, as a radio asking for its editor, still takes its
  // click, so it is chosen.
  if (element.matches("input")) cfg.preventTrigger = false;
  // A dialog's form closes it as it sends, so the swap never lands under
  // an open modal, unless it waits for the answer or the answer lands in
  // the dialog itself.
  const dialog = element.closest("dialog[open]");
  if (dialog && !dialog.contains(cfg.target) && !element.matches("[data-wait]")) {
    dialog.close();
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
    leave(response.url);
    return;
  }
  const away = response.headers.get("FX-Redirect");
  if (away) {
    event.preventDefault();
    leave(away);
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
        if (!element.isConnected || leaving) return;
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

// A browser without `commandfor`, as an older Safari, opens and closes
// dialogs from their buttons here instead, as the attributes would.
if (!("commandForElement" in HTMLButtonElement.prototype)) {
  document.addEventListener("click", (event) => {
    const button = event.target.closest("button[commandfor]");
    const dialog = button && document.getElementById(button.getAttribute("commandfor"));
    if (!(dialog instanceof HTMLDialogElement)) return;
    const command = button.getAttribute("command");
    if (command === "show-modal" && !dialog.open) dialog.showModal();
    if (command === "close") dialog.close();
  });
}

// A field with `data-caret-end`, as the download's file name, takes focus
// with the caret after what it holds, ready to add to it, as when its
// dialog opens on it; one clicked into keeps the caret where it was put.
let pressed = null;
addEventListener("pointerdown", (event) => (pressed = event.target), true);
document.addEventListener("focusin", (event) => {
  const field = event.target;
  const clicked = field === pressed;
  pressed = null;
  if (clicked || !field.matches("input[data-caret-end]")) return;
  field.setSelectionRange(field.value.length, field.value.length);
});

// A dialog's download form closes it once the download starts, since a
// download leaves the page where it is.
for (const dialog of document.querySelectorAll("dialog")) {
  dialog
    .querySelector('form[method="get"]')
    ?.addEventListener("submit", () => dialog.close());
}

// A palette's picker: choosing a block in its dialog shows it on the button
// that opened it and closes the dialog.
document.addEventListener("change", (event) => {
  const dialog = event.target.closest("dialog[data-picker]");
  if (!dialog || event.target.type !== "radio") return;
  const label = event.target.closest("label");
  const opener = document.querySelector(`[commandfor="${dialog.id}"][command="show-modal"]`);
  if (label && opener) {
    opener.querySelector("[data-picked-icon]").innerHTML = label.querySelector("svg").outerHTML;
    opener.querySelector("[data-picked-name]").textContent = label.dataset.name;
  }
  dialog.close();
});

// A setting edited in place: its edit button swaps its value for its form,
// and its cancel button swaps them back, the form as it was.
document.addEventListener("click", (event) => {
  const edit = event.target.closest("[data-edit]");
  const cancel = event.target.closest("[data-cancel]");
  const editable = (edit || cancel)?.closest("[data-editable]");
  if (!editable) return;
  const view = editable.querySelector("[data-view]");
  const editor = editable.querySelector("[data-editor]");
  view.hidden = Boolean(edit);
  editor.hidden = !edit;
  if (edit) {
    editor.querySelector("input:not([type=hidden])")?.focus();
  } else {
    editor.querySelector("form")?.reset();
    editor.querySelector("[role=alert]")?.remove();
    view.querySelector("[data-edit]")?.focus();
  }
});

// A form with `data-submit-on-pick` goes on as soon as a file is picked in
// it, from a list or from the computer, rather than waiting for a button.
document.addEventListener("change", (event) => {
  const form = event.target.closest("form[data-submit-on-pick]");
  if (form && (event.target.type === "radio" || event.target.type === "file")) {
    form.requestSubmit();
  }
});

// A disclosure button opens or closes what it controls, wherever a swap
// put it.
document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-disclose]");
  if (!button) return;
  const open = button.getAttribute("aria-expanded") !== "true";
  button.setAttribute("aria-expanded", String(open));
  for (const id of button.getAttribute("aria-controls").split(" ")) {
    const controlled = document.getElementById(id);
    if (controlled) controlled.hidden = !open;
  }
});

// A menu closes once a choice in it is made, or on a click outside it or
// Escape, as menus do.
const closeMenus = (except) => {
  for (const menu of document.querySelectorAll("details[data-menu][open]")) {
    if (menu !== except) menu.open = false;
  }
};
document.addEventListener("click", (event) => {
  const menu = event.target.closest("details[data-menu]");
  closeMenus(menu);
  if (menu && event.target.closest("a, button")) menu.open = false;
});
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  const open = document.querySelector("details[data-menu][open]");
  if (!open) return;
  open.open = false;
  open.querySelector("summary").focus();
});

// An upload's form gives way to its steps as soon as it sends, so the page
// answers before the file is in; coming back to the page shows the form.
document.addEventListener("submit", (event) => {
  const form = event.target.closest("form[data-analyzing]");
  const steps = document.querySelector("[data-uploading]");
  if (!form || !steps) return;
  form.hidden = true;
  steps.hidden = false;
});
window.addEventListener("pageshow", () => {
  for (const form of document.querySelectorAll("form[data-analyzing]")) {
    form.hidden = false;
  }
  for (const steps of document.querySelectorAll("[data-uploading]")) {
    steps.hidden = true;
  }
});

// A page marked `data-step-page` is a step on the way to another, as an
// edit page is to the build's, or a new version's check, after its edit
// page, is; a run of them, each come to from the one before, is kept, from
// the page before the first. Once a form marked `data-step` lands, the
// run is taken out of the history, and going back skips it: it goes back
// to the page the run started from when it lands there, as an edit lands
// on the build it started from, or else puts the page it lands on in the
// last one's place; a form on a page that is no step, as a build's delete
// is, takes out the page itself. It sends itself in the background to see where it lands; any
// answer but a redirect, as a form sent back with what to fix, it sends
// again as forms go, to show. A button naming a choice, as Check it's
// `action=file` does, sends it on to another step, and goes as forms go.
const STEPS = "steps";
const REDRAW = "redraw";
const steps = () => {
  try {
    return JSON.parse(sessionStorage.getItem(STEPS));
  } catch {
    return null;
  }
};
const keepSteps = (run) => {
  try {
    if (run) sessionStorage.setItem(STEPS, JSON.stringify(run));
    else sessionStorage.removeItem(STEPS);
  } catch {}
};
if (document.querySelector("[data-step-page]")) {
  const run = steps();
  // A page drawn again, as on a reload or a check finishing, is the same
  // step.
  if (run?.last !== location.href) {
    keepSteps(
      run?.last === document.referrer
        ? { ...run, depth: run.depth + 1, last: location.href }
        : { from: document.referrer, depth: 1, last: location.href },
    );
  }
}
document.addEventListener("submit", async (event) => {
  const form = event.target.closest("form[data-step]");
  if (!form || event.defaultPrevented || form.dataset.sent) return;
  if (event.submitter?.name) return;
  event.preventDefault();
  const submitter = event.submitter;
  const asForms = () => {
    form.dataset.sent = "yes";
    form.requestSubmit(submitter);
  };
  let response;
  try {
    // Its attribute, as a field named `action` hides the property.
    const action = new URL(form.getAttribute("action") ?? "", location.href);
    response = await fetch(action, {
      method: "post",
      body: new FormData(form, submitter),
    });
  } catch {
    return asForms();
  }
  if (!response.redirected) return asForms();
  const landed = response.url;
  const page = document.querySelector("[data-step-page]");
  const run = (page && steps()) || { from: document.referrer, depth: 1 };
  keepSteps(null);
  if (landed === run.from && history.length > run.depth) {
    // The page it goes back to shows what the steps changed.
    try {
      sessionStorage.setItem(REDRAW, landed);
    } catch {}
    history.go(-run.depth);
  } else {
    location.replace(landed);
  }
});
// A page gone back to after a step changed it is drawn again, should the
// browser have kept it as it was.
window.addEventListener("pageshow", (event) => {
  let redraw = null;
  try {
    redraw = sessionStorage.getItem(REDRAW);
    sessionStorage.removeItem(REDRAW);
  } catch {}
  if (event.persisted && redraw === location.href) location.reload();
});

// A link back to the listing a build was found in goes back through the
// history when that is where the page came from, keeping its place.
document.addEventListener("click", (event) => {
  const link = event.target.closest("a[data-back]");
  if (!link || event.button !== 0 || event.metaKey || event.ctrlKey) return;
  if (link.href === document.referrer && history.length > 1) {
    event.preventDefault();
    history.back();
  }
});

// A `<details>` the address points to, as a build's check a stage's link or
// a form's answer names, opens, so the place gone to shows what is there.
const openTarget = () => {
  const target = location.hash && document.getElementById(location.hash.slice(1));
  if (target instanceof HTMLDetailsElement) target.open = true;
};
window.addEventListener("hashchange", openTarget);
openTarget();

