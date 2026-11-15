"use strict";

// Search over the guides: every heading with the prose under it, scored by
// how many of the typed words it holds, headings weighing more than prose.
(() => {
  const box = document.querySelector(".search");
  if (!box) return;
  const root = box.dataset.root;
  const input = box.querySelector("input");
  const list = box.querySelector(".results");
  let entries = null;

  const load = async () => {
    entries ??= await fetch(`${root}search.json`).then((response) => response.json());
    return entries;
  };

  const score = (entry, words) => {
    const heading = `${entry.guide} ${entry.heading}`.toLowerCase();
    const text = entry.text.toLowerCase();
    let total = 0;
    for (const word of words) {
      if (heading.includes(word)) total += 3;
      else if (text.includes(word)) total += 1;
      else return 0;
    }
    return total;
  };

  const show = (found) => {
    list.replaceChildren();
    if (found.length === 0) {
      const empty = document.createElement("li");
      empty.className = "empty";
      empty.textContent = "Try other words";
      list.append(empty);
    }
    for (const entry of found) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = root + entry.url;
      link.textContent = entry.heading;
      const where = document.createElement("small");
      where.textContent = entry.guide;
      link.append(where);
      item.append(link);
      list.append(item);
    }
    list.hidden = false;
  };

  input.addEventListener("input", async () => {
    const words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0) {
      list.hidden = true;
      return;
    }
    const all = await load();
    const found = all
      .map((entry) => [score(entry, words), entry])
      .filter(([points]) => points > 0)
      .sort((a, b) => b[0] - a[0])
      .slice(0, 8)
      .map(([, entry]) => entry);
    show(found);
  });

  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      input.value = "";
      list.hidden = true;
    }
    if (event.key === "Enter") list.querySelector("a")?.click();
  });

  document.addEventListener("click", (event) => {
    if (!box.contains(event.target)) list.hidden = true;
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "/" && document.activeElement !== input) {
      event.preventDefault();
      input.focus();
    }
  });
})();
