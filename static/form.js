// Details forms. Without this script every field shows and the server
// checks them all; with it, fields follow the chosen category.

// Category fields: the category is typed, with a menu of the standard ones
// under their sections, and the fields follow the one it names, by label or
// name in any capitals. Each category's flags show only for it, the name box
// only for a category whose builds are named, the period only for a serial
// one, the capacity only for a memory, and the design list only once the
// category has designs, offering just those. Text naming no standard
// category is one off the list, with no width or encoding; that waits until
// the box is left, so the fields do not jump while a name is typed.
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
    form.querySelector("[data-named-field]").hidden = !has("data-named");
    form.querySelector("[data-period-field]").hidden = !has("data-periodic");
    form.querySelector("[data-bytes-field]").hidden = !has("data-holds-bytes");
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
  const pick = (option) => {
    input.value = option.dataset.value;
    close();
    update(true);
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
  const marker = toggle.form.querySelector('input[name="synchronized_unknown"]');
  toggle.indeterminate = true;
  toggle.addEventListener("change", () => marker?.remove(), { once: true });
}

// Build dates: picking a date unticks "Don't know", and ticking it
// clears the date.
for (const field of document.querySelectorAll("[data-date-field]")) {
  const date = field.querySelector('input[type="date"]');
  const unknown = field.querySelector('input[type="checkbox"]');
  date.addEventListener("input", () => {
    unknown.checked = date.value === "";
  });
  unknown.addEventListener("change", () => {
    if (unknown.checked) date.value = "";
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
