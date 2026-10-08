// Details forms. Without this script every field shows and the server
// checks them all; with it, fields follow the chosen category.

// Category fields: the name box shows only for other, the carry boxes only
// for categories with carries, and the design list only once the chosen
// category has designs, offering just those.
for (const select of document.querySelectorAll("select[data-category]")) {
  const form = select.closest("form");
  const name = form.querySelector("[data-category-name]");
  const nameInput = name.querySelector("input");
  const carries = form.querySelector("[data-carries-field]");
  const design = form.querySelector("select[data-design]");
  const designField = form.querySelector("[data-design-field]");

  const update = () => {
    const other = select.value === "other";
    name.hidden = !other;
    nameInput.required = other;
    // A category off the list has no width or encoding.
    for (const field of form.querySelectorAll("[data-standard-only]")) {
      field.hidden = other;
    }
    carries.hidden = !select.selectedOptions[0]?.hasAttribute("data-carries");
    form.querySelector("[data-named-field]").hidden =
      !select.selectedOptions[0]?.hasAttribute("data-named");
    let designs = false;
    for (const group of design.querySelectorAll("optgroup")) {
      const mine = group.dataset.for === select.value;
      group.hidden = !mine;
      group.disabled = !mine;
      designs ||= mine;
    }
    if (design.selectedOptions[0]?.disabled) design.value = "other";
    designField.hidden = !designs;
  };

  select.addEventListener("change", update);
  update();
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
      item.querySelector(".badge").classList.add(pill.class);
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
