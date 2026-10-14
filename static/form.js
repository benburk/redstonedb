// Details forms. Without this script every field shows and the server
// checks them all; with it, fields follow the chosen category.

// Category fields: the category is typed, with a menu of the standard ones
// under their sections, and the fields follow the one it names, by label or
// name in any capitals. Each category's flags show only for it, the name box
// only for a category whose builds are named, a measure such as a memory's
// capacity only for a category ranked by it, and the design list only once the category has designs, offering
// just those; the logic's first choice names the category's usual one. Text
// naming no standard category is one off the list, with no width, encoding,
// or throughput; that waits until the box is left, so the fields do not
// jump while a name is typed.
for (const input of document.querySelectorAll("input[data-category]")) {
  const form = input.closest("form");
  const menu = document.getElementById(input.getAttribute("aria-controls"));
  const options = [...menu.querySelectorAll('[role="option"]')];
  const groups = [...menu.querySelectorAll("[data-group]")];
  const noMatch = menu.querySelector("[data-no-match]");
  const design = form.querySelector("select[data-design]");
  const designField = form.querySelector("[data-design-field]");

  // The category the text names, as the server reads it.
  const chosen = () => {
    const text = input.value.trim().toLowerCase();
    return options.find(
      (option) =>
        option.dataset.value.toLowerCase() === text ||
        option.dataset.name === text.replaceAll(" ", "_"),
    );
  };

  const update = (settled) => {
    const option = chosen();
    if (!option && !settled) return;
    const name = option?.dataset.name ?? "";
    for (const field of form.querySelectorAll("[data-standard-only]")) {
      field.hidden = !option && input.value.trim() !== "";
    }
    for (const flags of form.querySelectorAll("[data-flags-for]")) {
      flags.hidden = flags.dataset.flagsFor !== name;
    }
    const has = (attribute) => option?.hasAttribute(attribute) ?? false;
    // A category such as adder reads its width and orientation from the
    // file, and has its stacking worked out by the simulation, which show in
    // place of the typed ones. A typed field no category hides comes back
    // when the category reads nothing.
    const decides = {
      width: "data-reads-file",
      orientation: "data-reads-file",
      encoding: "data-reads-file",
      stacking: "data-infers-stacking",
    };
    for (const [what, attribute] of Object.entries(decides)) {
      const derived = form.querySelector(`[data-derived-${what}]`);
      if (!derived) continue;
      const typed = form.querySelector(`[data-typed-${what}]`);
      derived.hidden = !has(attribute);
      if (!derived.hidden) typed.hidden = true;
      else if (!typed.hasAttribute("data-standard-only")) typed.hidden = false;
    }
    form.querySelector("[data-named-field]").hidden = !has("data-named");
    form.querySelector("[data-usual-logic]").textContent =
      `As the category usually is (${option?.dataset.logic ?? "unknown"})`;
    const measures = (option?.dataset.measures ?? "").split(" ");
    for (const field of form.querySelectorAll("[data-measure-field]")) {
      field.hidden = !measures.includes(field.dataset.measureField);
    }
    let designs = false;
    for (const group of design.querySelectorAll("optgroup")) {
      const mine = group.dataset.for === name;
      group.hidden = !mine;
      group.disabled = !mine;
      designs ||= mine;
    }
    if (design.selectedOptions[0]?.disabled) design.value = "other";
    designField.hidden = !designs;
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

// Stacking: the direction rows show only while Stackable is ticked.
for (const field of document.querySelectorAll("[data-stacking]")) {
  const toggle = field.querySelector('input[name="stackable"]');
  const rows = field.querySelector("[data-stack-rows]");
  const update = () => {
    rows.hidden = !toggle.checked;
  };
  toggle.addEventListener("change", update);
  update();
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

// Build dates, a box each for year, month, and day. A box that fills, or
// takes a dash, slash, dot, or space, moves on to the next; a month of 2 to
// 9 or a day of 4 to 9 cannot take a second digit, so it is filled out and
// moves on too. Backspace in an empty box, or an arrow at a box's end,
// moves back or on. A whole date pasted into any box fills all three.
// Leaving the boxes with a part missing or a day the calendar lacks says
// so beneath them. Typing a date unticks "Don't know", and ticking it
// clears the date.
for (const field of document.querySelectorAll("[data-date-field]")) {
  const parts = ["year", "month", "day"].map((name) =>
    field.querySelector(`input[name="built_on_${name}"]`),
  );
  const [year, month, day] = parts;
  const unknown = field.querySelector('input[type="checkbox"]');
  const message = field.querySelector("#built-on-message");
  const filledOut = { month: 1, day: 3 };

  const focus = (box, at) => {
    box.focus();
    const end = at === "start" ? 0 : box.value.length;
    box.setSelectionRange(end, at === "all" ? box.value.length : end);
  };
  const padded = (box) => {
    if (box !== year && /^[1-9]$/.test(box.value)) box.value = `0${box.value}`;
  };
  const problem = () => {
    const values = parts.map((box) => box.value);
    if (values.every((v) => v === "")) return "";
    if (values.some((v) => v === "")) return "Give the year, month, and day.";
    if (year.value.length !== 4) return "Give the year in four digits.";
    const [y, m, d] = values.map(Number);
    const date = new Date(Date.UTC(y, m - 1, d));
    const real =
      date.getUTCFullYear() === y &&
      date.getUTCMonth() === m - 1 &&
      date.getUTCDate() === d;
    if (!real) {
      return `There is no ${values[0]}-${values[1].padStart(2, "0")}-${values[2].padStart(2, "0")}.`;
    }
    if (date > new Date()) return "That date has not come yet.";
    return "";
  };
  const check = () => {
    const found = problem();
    message.textContent = found;
    message.hidden = found === "";
    message.setAttribute("role", found ? "alert" : "status");
    for (const box of parts) {
      box.toggleAttribute("aria-invalid", found !== "");
      box.setCustomValidity(found);
    }
  };
  const fill = (text) => {
    const found = text.trim().match(/^(\d{4})\D?(\d{1,2})\D?(\d{1,2})$/);
    if (!found) return false;
    [year.value, month.value, day.value] = found.slice(1);
    padded(month);
    padded(day);
    unknown.checked = false;
    check();
    focus(day, "end");
    return true;
  };

  parts.forEach((box, i) => {
    const next = parts[i + 1];
    const previous = parts[i - 1];
    box.addEventListener("input", (event) => {
      const moveOn = next && /[-/. ]$/.test(box.value) && box.value.length > 1;
      box.value = box.value.replace(/\D/g, "").slice(0, box.maxLength);
      unknown.checked = parts.every((b) => b.value === "");
      if (message.hidden === false) check();
      if (!next || event.inputType?.startsWith("delete")) return;
      const limit = filledOut[box.name.replace("built_on_", "")];
      if (limit !== undefined && /^\d$/.test(box.value) && Number(box.value) > limit) {
        padded(box);
      }
      if (moveOn || box.value.length === box.maxLength) focus(next, "all");
    });
    box.addEventListener("keydown", (event) => {
      const atStart = box.selectionStart === 0 && box.selectionEnd === 0;
      const atEnd = box.selectionStart === box.value.length;
      if (event.key === "Backspace" && previous && box.value === "") {
        event.preventDefault();
        focus(previous, "end");
      } else if (event.key === "ArrowLeft" && previous && atStart) {
        event.preventDefault();
        focus(previous, "end");
      } else if (event.key === "ArrowRight" && next && atEnd) {
        event.preventDefault();
        focus(next, "start");
      }
    });
    box.addEventListener("paste", (event) => {
      if (fill(event.clipboardData.getData("text"))) event.preventDefault();
    });
    box.addEventListener("blur", () => padded(box));
  });
  field.addEventListener("focusout", (event) => {
    if (!field.contains(event.relatedTarget)) check();
  });
  unknown.addEventListener("change", () => {
    if (!unknown.checked) return;
    for (const box of parts) box.value = "";
    check();
  });
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

// Simulation: the category and width choose a build's test, so the result
// under the form follows them, asked of the server whenever either
// changes: first the cases, then, once they pass, the slower timing. The
// edit page starts with the stored result, so it asks only once one
// changes. A category with no test answers at once, so the waiting note
// shows only for a run that takes a while. A passing result means latency
// and synchronized are measured, so their fields give way to a note.
//
// The checks panel follows along, and the button waits for the cases'
// result, though not the timing, which carries on after publishing. It
// also stays held for anything the server found, such as a duplicate,
// except broken conventions once “follows its own layout” is ticked.
for (const form of document.querySelectorAll("form[data-simulate]")) {
  const results = document.getElementById("simulation-results");
  const category = form.querySelector('input[name="category"]');
  const width = form.querySelector('input[name="width"]');
  const timingFields = form.querySelector("[data-timing-fields]");
  const timingNote = form.querySelector("[data-timing-note]");
  const button = form.querySelector("[data-publish]");
  const reason = form.querySelector("[data-gate-reason]");
  const unconventional = form.querySelector("[data-unconventional]");
  const blockedReason = reason.textContent;
  const key = () => `${category.value.trim().toLowerCase()}/${width.value}`;
  let asked = form.hasAttribute("data-simulated") ? key() : null;
  let controller = null;
  let simulating = false;

  const gate = () => {
    const kind = button.dataset.blocked;
    const blocked = kind !== undefined
      && !(kind === "conventions" && unconventional?.checked);
    button.disabled = blocked || simulating;
    reason.textContent = blocked
      ? blockedReason
      : simulating
      ? "Waiting for the simulation…"
      : "";
  };
  unconventional?.addEventListener("change", gate);

  // One line of the checks panel: its state, `pass`, `fail`, `run`, or
  // `wait`, and its words; no state hides it.
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
      state === "run" ? spinner.cloneNode() : { pass: "✓", fail: "✗", wait: "○" }[state],
    );
    line.querySelector("[data-text]").textContent = text;
  };
  const marked = (name, section, label) => {
    if (section) mark(name, section.dataset.state, `${label}: ${section.dataset.summary}`);
    else mark(name, null);
  };

  const say = (text, role) => {
    results.replaceChildren();
    const note = document.createElement("p");
    note.className = "mt-12 text-muted";
    if (role) note.setAttribute("role", role);
    note.textContent = text;
    results.append(note);
  };
  // A category with a test has its latency and synchronized measured, so
  // their fields give way from the start, and are not sent, unless the
  // simulation could not run the build, as for a block it does not model.
  // `fresh` is false while a new result is on its way. A new version's form
  // has no latency fields to give way.
  const chosenOption = () => {
    const typed = category.value.trim().toLowerCase();
    return [...form.querySelectorAll("[data-category-menu] [role=option]")]
      .find((option) => option.dataset.value.toLowerCase() === typed);
  };
  const measured = (fresh = true) => {
    found(fresh);
    if (!timingFields) return;
    const option = chosenOption();
    const tested = option?.hasAttribute("data-tested") ?? false;
    const shown = fresh ? results : null;
    const skipped = shown?.querySelector("[data-skipped]") != null;
    const passed = shown?.querySelector("[data-measures-timing]") != null;
    const measures = (tested && !skipped) || passed;
    timingFields.hidden = measures;
    timingNote.hidden = !measures;
    for (const input of timingFields.querySelectorAll("input")) {
      input.disabled = measures;
    }
    timingNote.querySelector("[data-measured-marker]").disabled = !measures;
  };
  // A register's stacking is the simulation's once it finds copies that
  // work, so its rows give way to the note then, and are not sent; until
  // then, and when it finds none, they are the builder's.
  const typedStacking = form.querySelector("[data-typed-stacking]");
  const derivedStacking = form.querySelector("[data-derived-stacking]");
  const found = (fresh) => {
    if (!typedStacking || !derivedStacking) return;
    if (!chosenOption()?.hasAttribute("data-finds-stacking")) {
      typedStacking.disabled = false;
      return;
    }
    const stacks = fresh && results.querySelector("[data-found-stacking]") != null;
    derivedStacking.hidden = !stacks;
    typedStacking.hidden = stacks;
    typedStacking.disabled = stacks;
  };
  const post = async (url, signal) => {
    const response = await fetch(url, {
      method: "POST",
      // Without a newly chosen file, which only choosing it sends.
      body: new URLSearchParams(
        [...new FormData(form)].filter(([, value]) => typeof value === "string"),
      ),
      signal,
    });
    if (!response.ok) throw new Error(response.statusText);
    return response.text();
  };
  const ask = async () => {
    if (key() === asked) return;
    asked = key();
    controller?.abort();
    controller = new AbortController();
    const { signal } = controller;
    simulating = true;
    gate();
    measured(false);
    mark("simulation", "run", "Simulating every case…");
    mark("timing", null);
    const waiting = setTimeout(() => say("Simulating every case…", "status"), 300);
    results.setAttribute("aria-busy", "true");
    try {
      results.innerHTML = await post(form.dataset.simulate, signal);
      clearTimeout(waiting);
      simulating = false;
      gate();
      measured();
      const section = results.querySelector("#simulation");
      if (!section) {
        mark(
          "simulation",
          "wait",
          category.value.trim() === ""
            ? "Choose a category to simulate it."
            : "No simulation for this category and width yet.",
        );
        return;
      }
      marked("simulation", section, "Simulation");
      const pending = results.querySelector("[data-timing-pending]");
      if (pending) {
        mark("timing", "run", "Measuring the timing, which carries on after you publish…");
        pending.outerHTML = await post(`${form.dataset.simulate}/timing`, signal);
      }
      marked("timing", results.querySelector("[data-timing]"), "Timing");
    } catch (error) {
      if (signal.aborted) return;
      simulating = false;
      gate();
      mark("simulation", "fail", "Could not simulate it.");
      say("Could not simulate it.");
    } finally {
      clearTimeout(waiting);
      if (!signal.aborted) results.removeAttribute("aria-busy");
    }
  };
  category.addEventListener("change", ask);
  width.addEventListener("change", ask);
  measured();
  gate();
  // The edit page's stored result may still be waiting for its timing.
  if (asked !== null && results.querySelector("[data-timing-pending]")) {
    asked = null;
  }
  ask();
}
