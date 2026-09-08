const { test } = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { runInNewContext } = require("node:vm");
const { join } = require("node:path");

const script = readFileSync(join(__dirname, "../shared/static/js/study.js"), "utf8");

function setup({ wide = false, hasAbstract = true, hasToc = true, hasOtherContent = false } = {}) {
  const moved = [];
  const abstract = { name: "abstract" };
  const toc = { name: "toc" };
  let removed = false;
  const frontmatter = {
    get textContent() { return hasOtherContent ? "Keep this source note" : ""; },
    remove() { removed = true; },
  };
  const introduction = { append(node) { moved.push(node); } };
  const navigation = { append(node) { moved.push(node); }, querySelectorAll: () => [] };
  const contents = { hidden: true, open: false, querySelector: () => navigation };
  const elements = {
    ".frontmatter > .abstract": hasAbstract ? abstract : null,
    ".study-introduction": introduction,
    ".frontmatter .toc": hasToc ? toc : null,
    ".study-contents": contents,
    ".frontmatter": frontmatter,
  };
  let resize;
  const media = {
    matches: wide,
    addEventListener(name, handler) { assert.equal(name, "change"); resize = handler; },
  };
  const page = { querySelector: (selector) => elements[selector], querySelectorAll: () => [] };
  runInNewContext(script, {
    document: { querySelector: () => page },
    matchMedia: () => media,
  });
  return { moved, abstract, toc, contents, get removed() { return removed; },
    resize(value) { media.matches = value; resize(); } };
}

test("composition moves the original source elements instead of copying prose", () => {
  const result = setup({ wide: true });
  assert.deepEqual(result.moved, [result.abstract, result.toc]);
  assert.equal(result.contents.hidden, false);
  assert.equal(result.contents.open, true);
  assert.equal(result.removed, true);
});

test("mobile contents start collapsed and respond to layout changes", () => {
  const result = setup();
  assert.equal(result.contents.open, false);
  result.resize(true);
  assert.equal(result.contents.open, true);
  result.resize(false);
  assert.equal(result.contents.open, false);
});

test("missing optional content does not create empty navigation", () => {
  const result = setup({ hasAbstract: false, hasToc: false });
  assert.equal(result.contents.hidden, true);
  assert.deepEqual(result.moved, []);
});

test("unmoved source material is preserved", () => {
  assert.equal(setup({ hasOtherContent: true }).removed, false);
});

test("other pages are left alone", () => {
  runInNewContext(script, {
    document: { querySelector: () => null },
    matchMedia() { throw new Error("must not run"); },
  });
});

function readingSetup(annotated = false) {
  const events = {};
  const frames = [];
  const attributes = () => ({
    attrs: {},
    setAttribute(key, value) { this.attrs[key] = value; },
    removeAttribute(key) { delete this.attrs[key]; },
  });
  const links = ["first", "second"].map(id => ({ ...attributes(), hash: "#" + id }));
  const headings = [400, 900].map(top => ({
    top,
    getBoundingClientRect() { return { top: this.top }; },
    closest(selector) { return selector === '.code-row' ? null : this; },
  }));
  const targets = { first: headings[0], second: headings[1] };
  const navigation = { append() {}, querySelectorAll: () => links };
  const contents = { hidden: true, querySelector: () => navigation };
  const code = { textContent: "first\n\n  <value>\tlast\n" };
  const pre = { id: "", querySelector: () => code };
  const status = { textContent: "" };
  let wrap;
  let wrapped = false;
  const actions = { prepend(button) { wrap = button; } };
  const listing = {
    id: "listing",
    querySelector(selector) {
      return { ".code-actions": actions, pre, ".code-status": status }[selector];
    },
    classList: { toggle(name, value) { assert.equal(name, "is-wrapped"); wrapped = value; } },
  };
  let scrollCount = 0;
  const note = {open: false};
  const row = {querySelectorAll: () => [note]};
  const line = {
    closest(selector) { return selector === ".code-row" ? (annotated ? row : null) : selector === ".code-gutter" ? {} : listing; },
    scrollIntoView() { scrollCount++; },
  };
  targets["listing-L1"] = line;
  let click;
  const page = {
    querySelector(selector) {
      return { ".frontmatter .toc": {}, ".study-contents": contents }[selector];
    },
    querySelectorAll: () => [listing],
    contains: target => headings.includes(target),
    addEventListener(name, handler) { assert.equal(name, "click"); click = handler; },
  };
  const view = {
    innerHeight: 800, location: { hash: "" },
    addEventListener(name, handler) { events[name] = handler; },
  };
  runInNewContext(script, {
    document: {
      querySelector: () => page,
      getElementById: id => targets[id],
      createElement() {
        return { ...attributes(), addEventListener(name, handler) { this[name] = handler; } };
      },
    },
    window: view,
    matchMedia: () => ({ matches: true, addEventListener() {} }),
    requestAnimationFrame: callback => frames.push(callback),
  });
  const flush = () => { while (frames.length) frames.shift()(); };
  flush();
  return { links, headings, pre, code, status, wrap, note,
    get wrapped() { return wrapped; }, get scrollCount() { return scrollCount; },
    scroll() { events.scroll(); flush(); },
    resize() { events.resize(); flush(); },
    hash(hash) { view.location.hash = hash; events.hashchange(); flush(); },
    clickLine() {
      click({ target: { closest: () => ({ hash: "#listing-L1" }) } });
      flush();
    },
  };
}

test("current-section marker tracks reading position in both directions", () => {
  const ui = readingSetup();
  assert.equal(ui.links[0].attrs["aria-current"], undefined);
  ui.headings[0].top = 100;
  ui.scroll();
  assert.equal(ui.links[0].attrs["aria-current"], "location");
  ui.headings[1].top = 150;
  ui.scroll();
  assert.equal(ui.links[0].attrs["aria-current"], undefined);
  assert.equal(ui.links[1].attrs["aria-current"], "location");
  ui.headings[1].top = 400;
  ui.resize();
  assert.equal(ui.links[0].attrs["aria-current"], "location");
  ui.headings[0].top = 500;
  ui.scroll();
  assert.equal(ui.links[0].attrs["aria-current"], undefined);
});

test("wrapping is local to presentation and leaves copyable text untouched", () => {
  const ui = readingSetup();
  const original = ui.code.textContent;
  assert.equal(ui.wrap.attrs["aria-controls"], ui.pre.id);
  assert.equal(ui.wrap.attrs["aria-pressed"], "false");
  ui.wrap.click();
  assert.equal(ui.wrapped, true);
  assert.equal(ui.wrap.attrs["aria-pressed"], "true");
  assert.match(ui.status.textContent, /Following a reference restores original line alignment/);
  assert.equal(ui.code.textContent, original);
  ui.wrap.click();
  assert.equal(ui.wrapped, false);
  assert.equal(ui.code.textContent, original);
});

test("line deep links restore original line alignment before scrolling", () => {
  const ui = readingSetup();
  ui.wrap.click();
  ui.hash("#listing-L1");
  assert.equal(ui.wrapped, false);
  assert.equal(ui.wrap.attrs["aria-pressed"], "false");
  assert.equal(ui.scrollCount, 1);
});

test("clicking an already-current line link also restores original line alignment", () => {
  const ui = readingSetup();
  ui.wrap.click();
  ui.clickLine();
  assert.equal(ui.wrapped, false);
  assert.equal(ui.scrollCount, 1);
});

test("unrelated, missing and malformed fragments do not disturb code wrapping", () => {
  const ui = readingSetup();
  ui.wrap.click();
  for (const hash of ["#first", "#missing", "#%invalid", ""]) ui.hash(hash);
  assert.equal(ui.wrapped, true);
  assert.equal(ui.scrollCount, 0);
});

test('annotated line links open notes without disabling wrapping', () => {
  const ui = readingSetup(true);
  ui.wrap.click();
  ui.hash('#listing-L1');
  assert.equal(ui.note.open, true);
  assert.equal(ui.wrapped, true);
  assert.equal(ui.scrollCount, 1);
  ui.note.open = false;
  ui.clickLine();
  assert.equal(ui.note.open, true);
  assert.equal(ui.wrapped, true);
});
