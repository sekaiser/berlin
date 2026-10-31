// Local filtering only. Without JavaScript, every bookmark and tag link remains.
function matchingReferences(items, query, selectedTags) {
  const words = normalizeReading(query).split(/\s+/).filter(Boolean);
  return items.filter(item => words.every(word => item.text.includes(word))
    && selectedTags.every(tag => item.tags.includes(tag)));
}
const normalizeReading = text => text.normalize('NFKC').toLocaleLowerCase().trim();
if (typeof module !== 'undefined') module.exports = {matchingReferences, normalizeReading};
if (typeof document !== 'undefined') document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll('[data-reading-list]').forEach(initializeReadingList);
});
function initializeReadingList(root, listIndex) {
  const form = root.querySelector('.reading-tools');
  const container = root.querySelector('.publication-resource-items');
  const entries = [...container.querySelectorAll('.publication-resource')];
  if (!form || !entries.length || form.dataset.initialized) return;
  form.dataset.initialized = 'true';
  const query = form.elements.q;
  const tagOptions = form.querySelector('.reading-tag-options');
  const count = form.querySelector('output');
  const reset = form.querySelector('button[type="reset"]');
  const empty = root.querySelector('.reading-no-results');
  const normalize = normalizeReading;
  const items = entries.map(element => ({element,
    text: normalize(element.textContent),
    tags: [...element.querySelectorAll('.paper-tag')].map(el => el.dataset.tag),
  }));
  const tags = [...new Set(items.flatMap(item => item.tags))].sort((a, b) => a.localeCompare(b));
  // Reuse the server-rendered slot; filter buttons match the corresponding links.
  const tagColors = new Map([...container.querySelectorAll('.paper-tag')]
    .map(tag => [tag.dataset.tag, tag.dataset.tagColor]));
  const parameters = new URL(location.href).searchParams;
  const queryKey = listIndex ? `q.${root.id}` : 'q';
  const tagKey = listIndex ? `tag.${root.id}` : 'tag';
  query.value = parameters.get(queryKey) || '';
  // Keep previously shared single-topic URLs usable; new links use repeated tag keys.
  const initialTags = new Set([...parameters.getAll(tagKey), ...(listIndex ? [] : parameters.getAll('topic'))]);
  const tagButtons = tags.map(tag => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'paper-tag';
    button.dataset.tag = tag;
    button.dataset.tagColor = tagColors.get(tag) || '0';
    button.setAttribute('aria-label', tag);
    button.setAttribute('aria-pressed', String(initialTags.has(tag)));
    button.setAttribute('aria-controls', container.id);
    const label = document.createElement('span');
    label.textContent = tag;
    const total = document.createElement('span');
    total.className = 'reading-tag-count';
    total.setAttribute('aria-hidden', 'true');
    const explanation = document.createElement('span');
    explanation.className = 'reading-tag-description';
    explanation.id = `${root.id}-tag-count-${tagOptions.children.length}`;
    button.setAttribute('aria-describedby', explanation.id);
    button.append(label, total, explanation);
    button.addEventListener('click', () => {
      button.setAttribute('aria-pressed', String(button.getAttribute('aria-pressed') !== 'true'));
      filter();
    });
    tagOptions.append(button);
    return button;
  });

  function filter(updateUrl = true) {
    const words = normalize(query.value).split(/\s+/).filter(Boolean);
    const selectedTags = tagButtons.filter(button => button.getAttribute('aria-pressed') === 'true').map(button => button.dataset.tag);
    const matchingText = items.filter(item => words.every(word => item.text.includes(word)));
    const matchesTags = (item, tags) => tags.every(tag => item.tags.includes(tag));
    const matches = new Set(matchingReferences(items, query.value, selectedTags));
    const shown = matches.size;
    for (const item of items) {
      item.element.hidden = !matches.has(item);
    }
    for (const button of tagButtons) {
      const tag = button.dataset.tag;
      const selected = selectedTags.includes(tag);
      const remaining = selected ? shown : matchingText.filter(item => matchesTags(item, [...selectedTags, tag])).length;
      button.querySelector('.reading-tag-count').textContent = remaining;
      button.querySelector('.reading-tag-description').textContent = selected
        ? `${remaining} matching references. Select to remove this tag filter.`
        : `${remaining} matching references if this tag is added.`;
    }
    count.textContent = `${shown} of ${items.length} saved references`;
    empty.hidden = shown !== 0;
    if (!shown) {
      const restrictions = [];
      if (selectedTags.length) restrictions.push(`all selected tags (${selectedTags.join(', ')})`);
      if (query.value.trim()) restrictions.push(`the search “${query.value.trim()}”`);
      const suggestion = selectedTags.length && query.value.trim() ? 'Deselect a tag or shorten the search.'
        : selectedTags.length ? 'Deselect a tag to broaden the results.' : 'Shorten the search or clear the filters.';
      empty.textContent = `No references match ${restrictions.join(' and ')}. ${suggestion}`;
    }
    reset.disabled = !query.value && !selectedTags.length;
    if (updateUrl) {
      const url = new URL(location.href);
      if (query.value) url.searchParams.set(queryKey, query.value); else url.searchParams.delete(queryKey);
      if (!listIndex) url.searchParams.delete('topic');
      url.searchParams.delete(tagKey);
      for (const tag of selectedTags) url.searchParams.append(tagKey, tag);
      history.replaceState(null, '', url);
    }
  }
  form.addEventListener('submit', event => { event.preventDefault(); filter(); });
  query.addEventListener('input', () => filter());
  form.addEventListener('reset', event => {
    event.preventDefault();
    query.value = '';
    for (const button of tagButtons) button.setAttribute('aria-pressed', 'false');
    filter();
    query.focus();
  });
  filter(false);
  form.hidden = false;
}
