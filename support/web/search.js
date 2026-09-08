/* Static publication search. No analytics, remote service, or source-file access. */
(function () {
  'use strict';
  const normalize = value => value.normalize('NFKC').toLowerCase();
  const termsFor = query => [...new Set(normalize(query.trim()).split(/\s+/u).filter(Boolean))];

  function prepare(index) {
    if (index.schema_version !== 1 || !Array.isArray(index.documents)) throw new Error('Unsupported search index');
    return index.documents.map(document => ({
      ...document,
      tagPaths: index.tag_paths || {},
      terms: [normalize(document.title), normalize(document.tags.join(' '))],
      sections: document.sections.map(section => ({
        ...section, terms: [normalize(section.heading), normalize(section.text)]
      }))
    }));
  }

  // All words must match a section or its document metadata. At most one result
  // per document: choose its strongest section instead of flooding the reader.
  function search(documents, query) {
    const terms = termsFor(query);
    if (!terms.length) return [];
    const results = [];
    for (const document of documents) {
      let best = null;
      for (const section of document.sections) {
        const fields = [document.terms[0], section.terms[0], document.terms[1], section.terms[1]];
        if (!terms.every(term => fields.some(field => field.includes(term)))) continue;
        const score = terms.reduce((sum, term) => sum + fields.reduce((value, field, i) => value + (field.includes(term) ? [8, 6, 4, 1][i] : 0), 0), 0);
        if (!best || score > best.score || (score === best.score && !best.section.text && section.text)) best = {document, section, score};
      }
      if (best) results.push(best);
    }
    return results.sort((a, b) => b.score - a.score || a.document.path.localeCompare(b.document.path));
  }

  function excerpt(text, query, size = 240) {
    // Use original text for display. Normalization can change string length, so
    // find a nearby word rather than slicing at a normalized string offset.
    const words = text.split(/\s+/u).filter(Boolean);
    if (!words.length || size < 1) return '';
    const terms = termsFor(query);
    const hit = words.findIndex(word => terms.some(term => normalize(word).includes(term)));
    let start = Math.max(0, hit);
    let before = 0;
    while (start > 0 && hit - start < 10 && before + Array.from(words[start - 1]).length + 1 <= size / 3) {
      before += Array.from(words[--start]).length + 1;
    }
    let end = start, length = 0;
    while (end < words.length) {
      const next = Array.from(words[end]).length + (end > start ? 1 : 0);
      // The limit is soft: retain a complete unusually long matching token.
      if (end > start && end > hit && length + next > size) break;
      length += next;
      end++;
    }
    return (start ? '…' : '') + words.slice(start, end).join(' ') + (end < words.length ? '…' : '');
  }

  function resultUrl(indexUrl, document, section) {
    // The index is at <publication>/search/index.json. Relative resolution also
    // works for repository GitHub Pages deployments, without assuming '/'.
    if (!/^notes\/[a-z0-9-]+\.html$/.test(document.path)) throw new Error('Invalid search result route');
    const url = new URL('../' + document.path, indexUrl);
    if (section.fragment) url.hash = section.fragment;
    return url.href;
  }

  function appendHighlighted(parent, text, query) {
    const terms = termsFor(query);
    // Mark complete matching tokens: normalization may change character offsets
    // (e.g. composed accents), but never changes the displayed source spelling.
    for (const token of text.split(/(\s+)/u).filter(Boolean)) {
      if (terms.some(term => normalize(token).includes(term))) {
        const mark = window.document.createElement('mark');
        mark.textContent = token;
        parent.append(mark);
      } else {
        parent.append(window.document.createTextNode(token));
      }
    }
  }

  function renderTags(note, indexUrl) {
    const tags = window.document.createElement('p');
    tags.className = 'search-result-tags';
    for (const name of note.tags) {
      const path = note.tagPaths[name];
      const linked = typeof path === 'string' && /^tags\/[a-z0-9-]+\.html$/.test(path);
      const tag = window.document.createElement(linked ? 'a' : 'span');
      tag.className = 'paper-tag';
      tag.dataset.tag = name.toLowerCase();
      if (linked) tag.href = new URL('../' + path, indexUrl).href;
      const label = window.document.createElement('span');
      label.textContent = name;
      tag.append(label);
      tags.append(tag);
    }
    return tags;
  }

  function renderResult({document: note, section}, indexUrl, query) {
    const item = window.document.createElement('li');
    const label = window.document.createElement('p');
    label.className = 'search-result-label';
    label.textContent = note.kind === 'guide' ? 'Guide' : 'Note';
    const heading = window.document.createElement('h2');
    const link = window.document.createElement('a');
    link.href = resultUrl(indexUrl, note, section);
    appendHighlighted(link, note.title, query);
    heading.append(link);
    item.append(label, heading);
    if (section.heading) {
      const context = window.document.createElement('p');
      context.className = 'search-result-section';
      appendHighlighted(context, 'In “' + section.heading + '”', query);
      item.append(context);
    }
    if (section.text) {
      const passage = window.document.createElement('p');
      passage.className = 'search-result-excerpt';
      appendHighlighted(passage, excerpt(section.text, query), query);
      item.append(passage);
    }
    if (note.tags.length) item.append(renderTags(note, indexUrl));
    return item;
  }

  async function mount(root) {
    const form = root.querySelector('form');
    const input = root.querySelector('input[type="search"]');
    const status = root.querySelector('[data-search-status]');
    const results = root.querySelector('[data-search-results]');
    const indexUrl = new URL(root.dataset.searchIndex, location.href);
    input.value = (new URL(location.href).searchParams.get('q') || '').slice(0, input.maxLength);
    status.textContent = 'Loading the notebook index…';
    let documents;
    try {
      const response = await fetch(indexUrl);
      if (!response.ok) throw new Error('Search index unavailable');
      documents = prepare(await response.json());
    } catch (_) {
      status.textContent = 'Search could not load. Reload to try again, or browse Notes below.';
      return;
    }
    const render = () => {
      const query = input.value.trim();
      const matches = search(documents, query);
      results.replaceChildren();
      if (!query) {
        status.textContent = 'Search titles, headings, text, code, and tags. All words must match.';
        return;
      }
      const visible = matches.slice(0, 30);
      status.textContent = matches.length
        ? `${matches.length} ${matches.length === 1 ? 'note' : 'notes'} found.${matches.length > 30 ? ' Showing the first 30; add words to narrow the results.' : ''}`
        : 'No matching notes. Try fewer words or another spelling.';
      for (const match of visible) results.append(renderResult(match, indexUrl, query));
    };
    const update = () => {
      const url = new URL(location.href);
      if (input.value.trim()) url.searchParams.set('q', input.value.trim()); else url.searchParams.delete('q');
      history.replaceState(null, '', url);
      render();
    };
    form.addEventListener('submit', event => { event.preventDefault(); update(); });
    input.addEventListener('input', update);
    window.addEventListener('popstate', () => {
      input.value = (new URL(location.href).searchParams.get('q') || '').slice(0, input.maxLength);
      render();
    });
    form.hidden = false;
    render();
  }

  if (typeof module !== 'undefined' && module.exports) module.exports = {prepare, search, excerpt, resultUrl};
  if (typeof document !== 'undefined') {
    for (const root of document.querySelectorAll('[data-search-index]')) mount(root);
  }
}());
