// Progressive enhancement: listings and line links work without this script.
document.addEventListener("DOMContentLoaded", () => {
  if (!navigator.clipboard?.writeText) return;

  for (const listing of document.querySelectorAll(".code-listing:not(.json-explorer .code-listing)")) {
    const button = listing.querySelector(".code-copy");
    const code = listing.querySelector("pre code");
    const source = listing.querySelector(".code-source");
    const status = listing.querySelector(".code-status");
    if (!button || !code || !status) continue;

    button.hidden = false;
    button.addEventListener("click", async () => {
      button.disabled = true;
      status.textContent = "";
      try {
        await navigator.clipboard.writeText(source ? JSON.parse(source.textContent) : code.textContent);
        status.textContent = "Code copied.";
      } catch {
        status.textContent = "Could not copy. Select the code to copy it manually.";
      } finally {
        button.disabled = false;
      }
    });
  }
});
