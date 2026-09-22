/** Language tab panels on the API page. */
(function () {
  const root = document.querySelector("[data-tabs]");
  if (!root) return;
  const buttons = [...root.querySelectorAll("[data-tab]")];
  const panels = [...root.querySelectorAll("[data-panel]")];

  function show(id) {
    for (const b of buttons) {
      b.setAttribute("aria-selected", String(b.dataset.tab === id));
    }
    for (const p of panels) {
      p.hidden = p.dataset.panel !== id;
    }
  }

  for (const b of buttons) {
    b.addEventListener("click", () => show(b.dataset.tab));
  }

  const hash = location.hash.replace("#", "");
  if (hash && buttons.some((b) => b.dataset.tab === hash)) show(hash);
  else if (buttons[0]) show(buttons[0].dataset.tab);
})();
