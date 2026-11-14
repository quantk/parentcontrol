// Small enhancements of the panel. Every page works without this file;
// it only adds conveniences. Handlers are delegated from the document, so
// content swapped in by htmx needs no setup.
"use strict";

(() => {
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
