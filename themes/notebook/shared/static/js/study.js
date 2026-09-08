// Article composition only. Source prose and navigation remain usable
// in their exported order when JavaScript is unavailable.
(() => {
  const page = document.querySelector(".study-page");
  if (!page) return;

  const abstract = page.querySelector(".frontmatter > .abstract");
  const introduction = page.querySelector(".study-introduction");
  if (abstract && introduction) introduction.append(abstract);

  const toc = page.querySelector(".frontmatter .toc");
  const contents = page.querySelector(".study-contents");
  const navigation = contents?.querySelector("nav");
  if (toc && contents && navigation) {
    navigation.append(toc);
    contents.hidden = false;
    const wide = matchMedia("(min-width: 1200px)");
    const updateContents = () => { contents.open = wide.matches; };
    updateContents();
    wide.addEventListener("change", updateContents);
    trackCurrentSection(page, navigation);
  }

  const frontmatter = page.querySelector(".frontmatter");
  if (frontmatter && !frontmatter.textContent.trim()) frontmatter.remove();

  addCodeWrapping(page);

  function fragmentTarget(hash) {
    if (!hash.startsWith("#") || hash.length === 1) return null;
    try {
      return document.getElementById(decodeURIComponent(hash.slice(1)));
    } catch {
      return null;
    }
  }

  function trackCurrentSection(page, navigation) {
    const sections = [...navigation.querySelectorAll('a[href^="#"]')]
      .map(link => ({ link, target: fragmentTarget(link.hash) }))
      .filter(({ target }) => target && page.contains(target))
      .map(({ link, target }) => ({
        link,
        heading: target.closest("h1, h2, h3, h4, h5, h6") || target,
      }));
    if (!sections.length) return;

    let pending = false;
    function update() {
      pending = false;
      const readingLine = Math.min(160, window.innerHeight * 0.25);
      let current = null;
      for (const section of sections) {
        if (section.heading.getBoundingClientRect().top <= readingLine) current = section;
      }
      for (const section of sections) {
        if (section === current) section.link.setAttribute("aria-current", "location");
        else section.link.removeAttribute("aria-current");
      }
    }
    function schedule() {
      if (pending) return;
      pending = true;
      requestAnimationFrame(update);
    }
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    window.addEventListener("load", schedule);
    document.fonts?.ready.then(schedule);
    update();
    schedule(); // Recheck after the remaining composition changes in this turn.
  }

  function addCodeWrapping(page) {
    const restore = new Map();
    for (const listing of page.querySelectorAll(".code-listing:not(.json-explorer .code-listing)")) {
      const actions = listing.querySelector(".code-actions");
      const pre = listing.querySelector("pre");
      const lines = listing.querySelector(".code-lines");
      const code = pre?.querySelector("code");
      const status = listing.querySelector(".code-status");
      if (!actions || !pre || !code || !status) continue;

      const button = document.createElement("button");
      button.type = "button";
      button.className = "code-wrap";
      button.textContent = "Wrap";
      button.setAttribute("aria-label", "Wrap code");
      button.setAttribute("aria-pressed", "false");
      const controlled = lines || pre;
      if (!controlled.id) controlled.id = listing.id + "-source";
      button.setAttribute("aria-controls", controlled.id);
      let wrapped = false;
      function setWrapped(value) {
        wrapped = value;
        listing.classList.toggle("is-wrapped", value);
        button.setAttribute("aria-pressed", String(value));
        status.textContent = value
          ? "Code wrapped. Following a reference restores original line alignment."
          : "Original line alignment restored.";
      }
      button.addEventListener("click", () => setWrapped(!wrapped));
      actions.prepend(button);
      restore.set(listing, () => {
        if (!wrapped) return false;
        setWrapped(false);
        return true;
      });
    }
    if (!restore.size) return;

    function restoreLine(hash, scroll) {
      const target = fragmentTarget(hash);
      const row = target?.closest(".code-row");
      if (row) {
        for (const note of row.querySelectorAll(".code-note")) note.open = true;
        if (scroll) requestAnimationFrame(() => target.scrollIntoView({ block: "center" }));
        return;
      }
      if (!target?.closest(".code-gutter")) return;
      const listing = target.closest(".code-listing");
      if (restore.get(listing)?.() && scroll) {
        requestAnimationFrame(() => target.scrollIntoView({ block: "center" }));
      }
    }
    // Also handle clicking an already-current fragment: no hashchange fires then.
    page.addEventListener("click", event => {
      const link = event.target.closest('a[href^="#"]');
      if (link) restoreLine(link.hash, true);
    });
    window.addEventListener("hashchange", () => restoreLine(window.location.hash, true));
    restoreLine(window.location.hash, true);
  }
})();
