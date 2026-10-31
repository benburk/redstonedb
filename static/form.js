// Details forms. Without this script every field shows and the server
// checks them all; with it, fields follow the chosen category.

// The option among a category menu's `options` that `text` names, by its
// label or its name in any capitals, as the server's `Category::find` reads
// it; none for a category off the list.
const namedCategory = (options, text) => {
  const typed = text.trim().toLowerCase();
  return options.find(
    (option) =>
      option.dataset.value.toLowerCase() === typed ||
      option.dataset.name === typed.replaceAll(" ", "_"),
  );
};

// Category fields: the category is typed, with a menu of the standard ones
// under their sections, and the fields follow the one it names, by label or
// name in any capitals. A blessed category, one probing recognizes, has
// every measured detail worked out from its file and by the simulation: its
// orientation, stacking, and logic give way to what the file shows, and a
// note says so. Any other name puts the build off the list, described by
// hand; that waits until the box is left, so the fields do not jump while a
// name is typed.
for (const input of document.querySelectorAll("input[data-category]")) {
  const form = input.closest("form");
  const menu = document.getElementById(input.getAttribute("aria-controls"));
  const options = [...menu.querySelectorAll('[role="option"]')];
  const groups = [...menu.querySelectorAll("[data-group]")];
  const noMatch = menu.querySelector("[data-no-match]");

  const chosen = () => namedCategory(options, input.value);

  const update = (settled) => {
    const option = chosen();
    if (!option && !settled) return;
    const has = (attribute) => option?.hasAttribute(attribute) ?? false;
    const blessed = has("data-blessed");
    for (const what of ["width", "orientation", "stacking"]) {
      const derived = form.querySelector(`[data-derived-${what}]`);
      const typed = form.querySelector(`[data-typed-${what}]`);
      if (derived) derived.hidden = !blessed;
      if (typed) typed.hidden = blessed;
    }
    // A category such as ALU is never stacked, so neither is asked.
    if (has("data-never-stacks")) {
      for (const field of form.querySelectorAll("[data-typed-stacking], [data-derived-stacking]")) {
        field.hidden = true;
      }
    }
    form.querySelector("[data-usual-logic]").textContent =
      `As the category usually is (${option?.dataset.logic ?? "unknown"})`;
    const logic = form.querySelector("[data-logic-field]");
    if (logic) logic.hidden = blessed;
    const note = form.querySelector("[data-evaluated-note]");
    if (note) note.hidden = !blessed;
  };

  // The menu: the categories whose labels hold the typed text, all of them
  // for an empty box or one that already names a category, so its
  // neighbours can be picked instead.
  const visible = () => options.filter((option) => !option.hidden);
  let active = null;
  const highlight = (option) => {
    active?.setAttribute("aria-selected", "false");
    active = option;
    if (option) {
      option.setAttribute("aria-selected", "true");
      input.setAttribute("aria-activedescendant", option.id);
      option.scrollIntoView({ block: "nearest" });
    } else {
      input.removeAttribute("aria-activedescendant");
    }
  };
  const filter = () => {
    const text = input.value.trim().toLowerCase();
    const all = text === "" || chosen() !== undefined;
    for (const option of options) {
      option.hidden = !all && !option.dataset.value.toLowerCase().includes(text);
    }
    for (const group of groups) {
      group.hidden = !group.querySelector('[role="option"]:not([hidden])');
    }
    noMatch.hidden = visible().length > 0;
    if (active?.hidden) highlight(null);
  };
  const open = () => {
    filter();
    menu.hidden = false;
    input.setAttribute("aria-expanded", "true");
  };
  const close = () => {
    menu.hidden = true;
    input.setAttribute("aria-expanded", "false");
    highlight(null);
  };
  // Picking is a change like typing and leaving the box, so whatever
  // follows the category hears of it.
  const pick = (option) => {
    input.value = option.dataset.value;
    close();
    input.dispatchEvent(new Event("change"));
  };

  input.addEventListener("focus", open);
  input.addEventListener("click", open);
  input.addEventListener("input", () => {
    open();
    highlight(null);
    update(false);
  });
  input.addEventListener("change", () => update(true));
  input.addEventListener("blur", close);
  input.addEventListener("keydown", (event) => {
    const shown = visible();
    const at = shown.indexOf(active);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (menu.hidden) open();
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = at === -1 ? (step === 1 ? 0 : shown.length - 1) : at + step;
      highlight(shown[Math.max(0, Math.min(shown.length - 1, next))] ?? null);
    } else if (event.key === "Enter" && !menu.hidden && active) {
      // Picks the highlighted category rather than sending the form.
      event.preventDefault();
      pick(active);
    } else if (event.key === "Escape" && !menu.hidden) {
      event.preventDefault();
      close();
    }
  });
  // Pressing, not clicking, so the box keeps its focus and does not close
  // the menu first.
  menu.addEventListener("mousedown", (event) => {
    event.preventDefault();
    const option = event.target.closest('[role="option"]');
    if (option) pick(option);
  });

  update(true);
}

// The synchronized toggle of a build where it is unknown shows as neither
// on nor off, and stays unknown until it is clicked.
for (const toggle of document.querySelectorAll("input[data-unknown]")) {
  const marker = toggle.form.querySelector(
    '[data-timing-fields] input[name="synchronized_unknown"]',
  );
  toggle.indeterminate = true;
  toggle.addEventListener("change", () => marker?.remove(), { once: true });
}

// Pill fields, for tags, builders, and contributors: pick a suggestion, or type a value
// and press Enter or comma, to add it as a pill. A value that is not a
// suggestion is added when it matches the field's pattern. Backspace in
// the empty box removes the last pill. Each pill carries a hidden input,
// so the chosen values are sent with the form.
for (const field of document.querySelectorAll("[data-pill-field]")) {
  const entry = field.querySelector("input[list]");
  const chosen = field.querySelector("[data-chosen]");
  const message = field.querySelector("[data-message]");
  const template = field.querySelector("[data-template]");
  const options = [...field.querySelectorAll("datalist option")];
  const pattern = new RegExp(field.dataset.pattern);
  const name = field.dataset.name;
  // The text box only feeds the pills once this script runs.
  entry.name = "";

  // The suggestion `text` names, by its label or its value.
  const find = (text) => {
    const wanted = text.trim().toLowerCase();
    return options.find(
      (option) =>
        option.value.toLowerCase() === wanted ||
        option.dataset.value === wanted.replaceAll(" ", "_"),
    );
  };

  // Adds the typed value; false when it cannot be added.
  const add = () => {
    message.textContent = "";
    const text = entry.value.trim();
    if (text === "") return true;
    const option = find(text);
    let pill;
    if (option) {
      pill = {
        label: option.value,
        value: option.dataset.value,
        class: option.dataset.class,
      };
    } else if (pattern.test(text)) {
      // Names keep their capitals; tags are lowercase words.
      const label = "keepCase" in field.dataset ? text : text.toLowerCase().replace(/\s+/g, " ");
      pill = {
        label,
        value: label.replaceAll(" ", "_"),
        class: field.dataset.newClass,
      };
    } else {
      message.textContent = `${text} ${field.dataset.hint}.`;
      return false;
    }
    const taken = [...chosen.querySelectorAll(`input[name="${name}"]`)];
    if (!taken.some((input) => input.value === pill.value)) {
      const item = template.content.firstElementChild.cloneNode(true);
      item.querySelector("[data-pill]").classList.add(pill.class);
      item.querySelector("[data-label]").textContent = pill.label;
      item.querySelector("[data-remove]").setAttribute("aria-label", `Remove ${pill.label}`);
      item.querySelector(`input[name="${name}"]`).value = pill.value;
      chosen.append(item);
    }
    entry.value = "";
    entry.focus();
    return true;
  };

  entry.addEventListener("keydown", (event) => {
    if (event.isComposing) return;
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      add();
    } else if (event.key === "Backspace" && entry.value === "") {
      chosen.lastElementChild?.remove();
    }
  });
  // Picking a suggestion replaces the text in one step; add it at once.
  entry.addEventListener("input", (event) => {
    if (event.inputType === "insertReplacementText" || !(event instanceof InputEvent)) {
      add();
    }
  });
  chosen.addEventListener("click", (event) => {
    const remove = event.target.closest("[data-remove]");
    if (!remove) return;
    remove.closest("li").remove();
    entry.focus();
  });
  // A value typed but not yet added still goes with the form.
  field.closest("form").addEventListener("submit", (event) => {
    if (!add()) event.preventDefault();
  });
}

// A link to the file chooser opens it, folded away as it starts.
for (const link of document.querySelectorAll("a[data-open-chooser]")) {
  link.addEventListener("click", () => {
    const chooser = document.getElementById("file-chooser");
    if (chooser) chooser.open = true;
  });
}

// Simulation: the category chooses a build's test, so the result under the
// form, a live panel, is redrawn whenever it changes: first
// the cases, then, once they pass, the slower timing, which the cases'
// section loads by itself. The edit page starts with the stored result, so
// it redraws only once one changes. A category with no test answers at
// once, so the waiting note shows only for a run that takes a while. A
// passing result means latency and synchronized are measured, so their
// fields give way to a note.
//
// The checks panel follows along, saying whether the build goes up as its
// category or off the list, and the button waits for the cases' result,
// though not the timing, which carries on after publishing. It also stays
// held for anything the server found, such as a duplicate.
for (const form of document.querySelectorAll("form[data-simulate]")) {
  const results = () => document.getElementById("simulation-results");
  const category = form.querySelector('input[name="category"]');
  const timingFields = form.querySelector("[data-timing-fields]");
  const timingNote = form.querySelector("[data-timing-note]");
  const button = form.querySelector("[data-publish]");
  const reason = form.querySelector("[data-gate-reason]");
  const blockedReason = reason.textContent;
  const key = () => category.value.trim().toLowerCase();
  let asked = form.hasAttribute("data-simulated") ? key() : null;
  // Requests for the cases under way; the button waits on them.
  let running = 0;

  const gate = () => {
    const blocked = button.dataset.blocked !== undefined;
    button.disabled = blocked || running > 0;
    reason.textContent = blocked
      ? blockedReason
      : running > 0
      ? "Waiting for the simulation…"
      : "";
  };

  // One line of the checks panel: its state, `pass`, `warn`, `fail`,
  // `run`, or `wait`, and its words; no state hides it.
  const spinner = document.createElement("span");
  spinner.className =
    "inline-block size-4 shrink-0 animate-spin rounded-full border-2 border-line border-t-brand motion-reduce:animate-none";
  const mark = (name, state, text) => {
    const line = document.querySelector(`[data-check="${name}"]`);
    if (!line) return;
    line.hidden = !state;
    if (!state) return;
    line.dataset.state = state;
    line.classList.toggle("text-danger", state === "fail");
    line.classList.toggle("text-muted", state === "run" || state === "wait");
    const icon = line.querySelector("[data-icon]");
    icon.replaceChildren(
      state === "run" ? spinner.cloneNode() : { pass: "✓", warn: "!", fail: "✗", wait: "○" }[state],
    );
    line.querySelector("[data-text]").textContent = text;
  };
  const marked = (name, section, label) => {
    if (section) mark(name, section.dataset.state, `${label}: ${section.dataset.summary}`);
    else mark(name, null);
  };

  // A category with a test has its latency and synchronized measured, so
  // their fields give way from the start, and are not sent, unless the
  // simulation could not run the build, as for a block it does not model.
  // `fresh` is false while a new result is on its way. A new version's form
  // has no latency fields to give way.
  const categoryOptions = [...form.querySelectorAll("[data-category-menu] [role=option]")];
  const chosenOption = () => namedCategory(categoryOptions, category.value);
  const measured = (fresh = true) => {
    found(fresh);
    if (!timingFields) return;
    const option = chosenOption();
    const tested = option?.hasAttribute("data-blessed") ?? false;
    const shown = fresh ? results() : null;
    const skipped = shown?.querySelector("[data-skipped]") != null;
    const passed = shown?.querySelector("[data-measures-timing]") != null;
    // One that fails goes up off the list, described by hand.
    const ran = shown?.querySelector("#simulation") != null;
    const measures = ran ? passed : tested && !skipped;
    timingFields.hidden = measures;
    timingNote.hidden = !measures;
    for (const input of timingFields.querySelectorAll("input")) {
      input.disabled = measures;
    }
    timingNote.querySelector("[data-measured-marker]").disabled = !measures;
  };
  // A blessed build's stacking is the simulation's, which the note shows
  // once it has run.
  const derivedStacking = form.querySelector("[data-derived-stacking]");
  const found = (fresh) => {
    if (!derivedStacking) return;
    const stack = fresh ? results().querySelector("[data-stack]")?.dataset.stack : null;
    derivedStacking.querySelector("[data-stack-note]").textContent =
      stack ?? "Found by the simulation once it has run.";
  };

  // The checks once a result is in.
  const settled = () => {
    measured();
    const filing = results().querySelector("[data-filing]");
    if (filing) mark("filing", filing.dataset.filing, filing.textContent);
    else mark("filing", null);
    const section = results().querySelector("#simulation");
    if (!section) {
      mark(
        "simulation",
        "wait",
        category.value.trim() === ""
          ? "Choose a category to simulate it."
          : "Only adders, ALUs, multipliers, incrementers, decrementers, comparators, carry outs, absolute values, popcounts, thresholds, binary-to-hex converters, gates, inverters, decoders, barrel shifters, monostables, registers, buffers, counters, and shift registers are simulated so far.",
      );
      mark("timing", null);
      return;
    }
    marked("simulation", section, "Simulation");
    if (!section.querySelector("[data-timing-pending]")) {
      marked("timing", section.querySelector("[data-timing]"), "Timing");
    }
  };

  let waiting = null;
  document.addEventListener("fx:before", (event) => {
    if (event.target.id === "simulation-results") {
      running += 1;
      gate();
      measured(false);
      mark("simulation", "run", "Simulating every case…");
      mark("timing", null);
      mark("filing", null);
      clearTimeout(waiting);
      waiting = setTimeout(() => {
        const note = document.createElement("p");
        note.className = "mt-12 text-muted";
        note.textContent = "Simulating every case…";
        results().replaceChildren(note);
      }, 300);
    } else if (event.target.matches("[data-timing-pending]")) {
      mark("timing", "run", "Measuring the timing, which carries on after you publish…");
    }
  });
  document.addEventListener("fx:finally", (event) => {
    if (event.target.id !== "simulation-results") return;
    running -= 1;
    if (running === 0) clearTimeout(waiting);
    gate();
  });
  document.addEventListener("fx:error", (event) => {
    if (event.detail.error?.name === "AbortError") return;
    if (event.target.id === "simulation-results") {
      mark("simulation", "fail", "Could not simulate it.");
    } else if (event.target.matches("[data-timing-pending]")) {
      mark("timing", "fail", "Could not measure the timing.");
    }
  });
  // A swap replaces its target, so fixi tells the document, with the
  // target it replaced.
  document.addEventListener("fx:swapped", (event) => {
    const { cfg } = event.detail;
    if (cfg.target.id === "simulation-results") settled();
    else if (cfg.target.matches?.("[data-timing-pending]")) {
      marked("timing", results().querySelector("[data-timing]"), "Timing");
    }
  });

  const ask = () => {
    if (key() === asked) return;
    asked = key();
    results().dispatchEvent(new Event("redraw"));
  };
  category.addEventListener("change", ask);
  measured();
  gate();
  // fixi takes up the panel on DOMContentLoaded, after this script runs.
  document.addEventListener("DOMContentLoaded", ask);
}
