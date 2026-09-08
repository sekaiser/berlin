// No giscus script, iframe, preconnect, or storage access before the reader opts in.
document.addEventListener('DOMContentLoaded', () => {
  const section = document.querySelector('.article-discussion');
  if (!section) return;
  const button = section.querySelector('.discussion-load');
  const status = section.querySelector('.discussion-status');
  const container = section.querySelector('.giscus');
  let started = false;
  button.hidden = false;
  button.addEventListener('click', () => {
    if (started) return;
    started = true;
    button.disabled = true;
    status.textContent = 'Loading discussion… You can also open it on GitHub.';
    const script = document.createElement('script');
    script.src = 'https://giscus.app/client.js';
    script.async = true;
    script.crossOrigin = 'anonymous';
    for (const key of ['repo', 'repoId', 'category', 'categoryId', 'term', 'theme']) script.dataset[key] = section.dataset[key];
    Object.assign(script.dataset, {mapping: 'specific', strict: '1', reactionsEnabled: '0', emitMetadata: '0', inputPosition: 'top', lang: 'en'});
    const failed = () => {
      started = false;
      script.remove();
      button.disabled = false;
      button.hidden = false;
      button.textContent = 'Retry loading';
      status.textContent = 'The discussion could not be loaded. Retry or use the GitHub link.';
    };
    script.addEventListener('error', failed, {once: true});
    script.addEventListener('load', () => {
      button.hidden = true;
      status.textContent = 'If comments do not appear, use the GitHub link above.';
    }, {once: true});
    // The official client owns iframe creation, authentication, resizing and errors.
    container.after(script);
  });
});
