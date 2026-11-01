// Local enhancements only: sharing links work without JavaScript or platform SDKs.
(() => {
  const page = document.querySelector(".study-page");
  const utilities = page?.querySelector(".article-utilities");
  if (!utilities) return;

  const duration = utilities.querySelector(".article-reading-time");
  const prose = page.querySelector(".study-prose");
  if (duration && prose) {
    const content = prose.cloneNode(true);
    for (const element of content.querySelectorAll("pre, .code-listing, script, style, nav, .toc, iframe")) element.remove();
    const introduction = page.querySelector(".study-introduction .abstract");
    const text = [introduction?.textContent || "", content.textContent].join(" ");
    const words = text.match(/[\p{L}\p{N}]+(?:[’'-][\p{L}\p{N}]+)*/gu) || [];
    if (words.length) {
      const minutes = Math.max(1, Math.ceil(words.length / 220));
      duration.querySelector("[data-reading-label]").textContent = `${minutes} min`;
      duration.setAttribute("aria-label", `Estimated reading time: ${minutes} minutes`);
      duration.title = "Estimated at 220 words per minute, excluding code listings.";
      duration.hidden = false;
    }
  }

  const button = utilities.querySelector(".article-copy-link");
  const status = utilities.querySelector(".article-action-status");
  const url = utilities.dataset.articleUrl;
  if (!button || !status || !url || !navigator.clipboard?.writeText) return;
  button.hidden = false;
  button.addEventListener("click", async () => {
    button.disabled = true;
    status.textContent = "";
    try {
      await navigator.clipboard.writeText(url);
      status.textContent = "Link copied.";
    } catch {
      status.textContent = "Could not copy. Copy the address from your browser.";
    } finally {
      button.disabled = false;
    }
  });
})();
