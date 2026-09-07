// Progressive enhancement: listings and line links work without this script.
document.addEventListener("DOMContentLoaded", () => {
  if (!navigator.clipboard?.writeText) return;

  for (const listing of document.querySelectorAll(".code-listing")) {
    const button = listing.querySelector(".code-copy");
    const code = listing.querySelector("pre code");
    const status = listing.querySelector(".code-status");
    if (!button || !code || !status) continue;

    button.hidden = false;
    button.addEventListener("click", async () => {
      button.disabled = true;
      status.textContent = "";
      try {
        await navigator.clipboard.writeText(code.textContent);
        status.textContent = "Code copied.";
      } catch {
        status.textContent = "Could not copy. Select the code to copy it manually.";
      } finally {
        button.disabled = false;
      }
    });
  }
});
