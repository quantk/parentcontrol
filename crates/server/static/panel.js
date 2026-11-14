// Small enhancements of the panel. Every page works without this file;
// it only adds conveniences. Handlers are delegated from the document, so
// content swapped in by htmx needs no setup.
"use strict";

(() => {
  // Controls that need this file are shown only when it runs.
  document.documentElement.classList.add("js");

  // ---------- Flash messages ----------

  const FLASH_TIMEOUT_MS = 5000;

  const dismiss = (flash) => {
    flash.classList.add("leaving");
    flash.addEventListener("transitionend", () => flash.remove(), { once: true });
    // Without transitions (reduced motion) there is no transitionend.
    setTimeout(() => flash.remove(), 400);
  };

  // Confirmations fade on their own; errors stay until closed.
  const scheduleFlash = (root) => {
    for (const flash of root.querySelectorAll(".flash-done:not([data-timed])")) {
      flash.dataset.timed = "";
      setTimeout(() => dismiss(flash), FLASH_TIMEOUT_MS);
    }
  };

  document.addEventListener("click", (event) => {
    const close = event.target.closest("[data-dismiss]");
    if (close) dismiss(close.closest(".flash"));
  });

  // ---------- Rule form ----------

  // The value field explains what the chosen kind of rule expects, and is
  // off for kinds that need none.
  document.addEventListener("change", (event) => {
    const kind = event.target.closest("select[data-rule-kind]");
    if (!kind) return;
    const value = kind.form.elements.namedItem("value");
    const placeholder = kind.selectedOptions[0]?.dataset.placeholder ?? "";
    value.placeholder = placeholder;
    value.disabled = kind.value === "minecraft_java";
    if (value.disabled) value.value = "";
  });

  // ---------- Schedule editor ----------

  const MINUTES_PER_DAY = 24 * 60;
  const WEEKDAYS = ["monday", "tuesday", "wednesday", "thursday", "friday"];
  const WEEKEND = ["saturday", "sunday"];

  const minutesOf = (text) => {
    const match = /^(\d\d):(\d\d)$/.exec(text);
    return match ? Number(match[1]) * 60 + Number(match[2]) : null;
  };

  // Mirrors DayField::preview on the server: allowed stretches of the
  // budget day, in percent.
  const segmentsOf = (day, dayStart) => {
    if (day.querySelector('input[value="all"]').checked) return [[0, 100]];
    const segments = [];
    for (const row of day.querySelectorAll(".window-row")) {
      const [startField, endField] = row.querySelectorAll('input[type="time"]');
      const start = minutesOf(startField.value);
      const end = minutesOf(endField.value);
      if (start === null || end === null || start === end) continue;
      const from = (start - dayStart + MINUTES_PER_DAY) % MINUTES_PER_DAY;
      const to = (end - dayStart + MINUTES_PER_DAY) % MINUTES_PER_DAY || MINUTES_PER_DAY;
      if (from >= to) continue;
      segments.push([(from / MINUTES_PER_DAY) * 100, ((to - from) / MINUTES_PER_DAY) * 100]);
    }
    return segments;
  };

  const redrawStrip = (day) => {
    const form = day.closest("form.schedule");
    const dayStart = minutesOf(form.dataset.dayStart) ?? 0;
    const strip = day.querySelector(".day-strip");
    strip.replaceChildren(
      ...segmentsOf(day, dayStart).map(([from, length]) => {
        const segment = document.createElement("span");
        segment.style.setProperty("--from", `${from.toFixed(1)}%`);
        segment.style.setProperty("--length", `${length.toFixed(1)}%`);
        return segment;
      }),
    );
  };

  const markUnsaved = (form) => {
    const note = form.querySelector("[data-unsaved]");
    if (note) note.hidden = false;
  };

  const newWindowRow = (key, start = "", end = "") => {
    const row = document.getElementById("window-row").content.firstElementChild.cloneNode(true);
    const [startField, endField] = row.querySelectorAll('input[type="time"]');
    startField.name = `start_${key}`;
    endField.name = `end_${key}`;
    startField.value = start;
    endField.value = end;
    return row;
  };

  const windowsOf = (day) => day.querySelector(".day-windows");

  const copyDay = (source, target) => {
    target.querySelector('input[type="number"]').value =
      source.querySelector('input[type="number"]').value;
    const mode = source.querySelector('input[type="radio"]:checked').value;
    target.querySelector(`input[type="radio"][value="${mode}"]`).checked = true;
    const rows = [...source.querySelectorAll(".window-row")].map((row) => {
      const [start, end] = row.querySelectorAll('input[type="time"]');
      return newWindowRow(target.dataset.day, start.value, end.value);
    });
    for (const row of target.querySelectorAll(".window-row")) row.remove();
    windowsOf(target).prepend(...rows);
    redrawStrip(target);
  };

  document.addEventListener("click", (event) => {
    const add = event.target.closest("[data-add-window]");
    if (add) {
      const day = add.closest(".schedule-day");
      const row = newWindowRow(day.dataset.day);
      add.before(row);
      row.querySelector("input").focus();
      markUnsaved(day.closest("form"));
      return;
    }

    const remove = event.target.closest("[data-remove-window]");
    if (remove) {
      const day = remove.closest(".schedule-day");
      remove.closest(".window-row").remove();
      // Keep a place to type a window.
      if (!day.querySelector(".window-row")) {
        day.querySelector("[data-add-window]").before(newWindowRow(day.dataset.day));
      }
      redrawStrip(day);
      markUnsaved(day.closest("form"));
      return;
    }

    const copy = event.target.closest("[data-copy-to]");
    if (copy) {
      const source = copy.closest(".schedule-day");
      const form = source.closest("form");
      const targets = { weekdays: WEEKDAYS, weekend: WEEKEND, all: [...WEEKDAYS, ...WEEKEND] }[
        copy.dataset.copyTo
      ];
      for (const key of targets) {
        const target = form.querySelector(`.schedule-day[data-day="${key}"]`);
        if (target && target !== source) copyDay(source, target);
      }
      copy.closest("details").open = false;
      markUnsaved(form);
    }
  });

  document.addEventListener("input", (event) => {
    const day = event.target.closest(".schedule-day");
    if (!day) return;
    redrawStrip(day);
    markUnsaved(day.closest("form"));
  });

  // ---------- Menus ----------

  // An open menu closes on a click elsewhere.
  document.addEventListener("click", (event) => {
    for (const menu of document.querySelectorAll("details.menu[open]")) {
      if (!menu.contains(event.target)) menu.open = false;
    }
  });

  // ---------- Polling ----------

  // Parts of a page marked with data-poll refresh only while it is visible.
  document.addEventListener("htmx:beforeRequest", (event) => {
    if (document.hidden && event.detail.elt.closest("[data-poll]")) {
      event.preventDefault();
    }
  });

  // ---------- Tooltips ----------

  // Chart marks carry their description in data-tip; it shows on hover,
  // keyboard focus and tap.
  let tooltip = null;
  let owner = null;

  const hideTip = () => {
    tooltip?.remove();
    tooltip = null;
    owner = null;
  };

  const showTip = (element) => {
    if (owner === element) return;
    hideTip();
    owner = element;
    tooltip = document.createElement("div");
    tooltip.className = "tooltip";
    tooltip.setAttribute("role", "tooltip");
    for (const line of element.dataset.tip.split("\n")) {
      const row = document.createElement("div");
      row.textContent = line;
      tooltip.append(row);
    }
    document.body.append(tooltip);

    const mark = element.getBoundingClientRect();
    const tip = tooltip.getBoundingClientRect();
    const margin = 8;
    let left = mark.left + mark.width / 2 - tip.width / 2;
    left = Math.max(margin, Math.min(left, window.innerWidth - tip.width - margin));
    let top = mark.top - tip.height - margin;
    if (top < margin) top = mark.bottom + margin;
    tooltip.style.left = `${left + window.scrollX}px`;
    tooltip.style.top = `${top + window.scrollY}px`;
  };

  document.addEventListener("pointerover", (event) => {
    const element = event.target.closest("[data-tip]");
    if (element) showTip(element);
    else if (event.pointerType === "mouse") hideTip();
  });

  document.addEventListener("focusin", (event) => {
    const element = event.target.closest("[data-tip]");
    if (element) showTip(element);
    else hideTip();
  });

  document.addEventListener("pointerdown", (event) => {
    if (!event.target.closest("[data-tip]")) hideTip();
  });

  window.addEventListener("scroll", hideTip, { passive: true });

  // ---------- Page lifecycle ----------

  document.addEventListener("htmx:beforeSwap", hideTip);
  document.addEventListener("htmx:load", (event) => scheduleFlash(event.detail.elt));
  document.addEventListener("DOMContentLoaded", () => scheduleFlash(document));
})();
