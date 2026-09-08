// Local filtering only. Without JavaScript, every bookmark and tag link remains.
document.addEventListener('DOMContentLoaded', () => {
  const form = document.querySelector('.reading-tools');
  const entries = [...document.querySelectorAll('#reading-entries .journal-bookmark')];
  if (!form || !entries.length) return;
  const query = form.elements.q;
  const tagOptions = form.querySelector('.reading-tag-options');
  const count = form.querySelector('output');
  const reset = form.querySelector('button[type="reset"]');
  const empty = document.querySelector('.reading-no-results');
  const normalize = text => text.normalize('NFKC').toLocaleLowerCase().trim();
  const items = entries.map(element => ({element,
    text: normalize([...element.querySelectorAll('h2, .journal-label, .journal-summary p, .journal-annotation p, .journal-topics')].map(el => el.textContent).join(' ')),
    tags: [...element.querySelectorAll('.paper-tag')].map(el => el.dataset.tag),
  }));
  const tags = [...new Set(items.flatMap(item => item.tags))].sort((a, b) => a.localeCompare(b));
  const parameters = new URL(location.href).searchParams;
  query.value = parameters.get('q') || '';
  // Keep previously shared single-topic URLs usable; new links use repeated tag keys.
  const initialTags = new Set([...parameters.getAll('tag'), ...parameters.getAll('topic')]);
  const tagButtons = tags.map(tag => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'paper-tag';
    button.dataset.tag = tag;
    button.setAttribute('aria-label', tag);
    button.setAttribute('aria-pressed', String(initialTags.has(tag)));
    button.setAttribute('aria-controls', 'reading-entries');
    const label = document.createElement('span');
    label.textContent = tag;
    const total = document.createElement('span');
    total.className = 'reading-tag-count';
    total.setAttribute('aria-hidden', 'true');
    const explanation = document.createElement('span');
    explanation.className = 'reading-tag-description';
    explanation.id = `reading-tag-count-${tagOptions.children.length}`;
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
    const matches = new Set(matchingText.filter(item => matchesTags(item, selectedTags)));
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
      if (query.value) url.searchParams.set('q', query.value); else url.searchParams.delete('q');
      url.searchParams.delete('topic');
      url.searchParams.delete('tag');
      for (const tag of selectedTags) url.searchParams.append('tag', tag);
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
});
