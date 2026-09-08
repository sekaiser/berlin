// Local preview only. A successful watched build advances the server revision.
(() => {
  let previous;
  async function poll() {
    try {
      if (!document.hidden) {
        const response = await fetch('/__berlin/revision', {cache: 'no-store'});
        if (response.ok) {
          const revision = await response.text();
          if (previous !== undefined && revision !== previous) location.reload();
          previous = revision;
        }
      }
    } catch (_) {
      // A restarting preview server should not disrupt the current page.
    } finally {
      setTimeout(poll, 1000);
    }
  }
  poll();
})();
