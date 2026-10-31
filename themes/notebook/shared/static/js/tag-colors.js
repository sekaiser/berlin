// Match the template's stable_hash (UTF-8 FNV-1a), then select one of 64 slots.
// Theme-only hook: the shared search client also works without this palette.
(function (root) {
  'use strict';
  function index(label) {
    let hash = 2166136261;
    for (const byte of new TextEncoder().encode(label.toLowerCase())) {
      hash = Math.imul(hash ^ byte, 16777619) >>> 0;
    }
    return hash % 64;
  }
  root.NotebookTagColors = {index};
  if (typeof module !== 'undefined') module.exports = {index};
})(globalThis);
