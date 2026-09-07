const { test } = require("node:test");
const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { runInNewContext } = require("node:vm");
const { join } = require("node:path");

const script = readFileSync(join(__dirname, "code-controls.js"), "utf8");

function setup(clipboard) {
  const events = {};
  const button = {
    hidden: true,
    disabled: false,
    addEventListener: (name, handler) => { events[name] = handler; },
  };
  const code = { textContent: 'first\n\n<script>&\tlast\n' };
  const status = { textContent: "" };
  const elements = { ".code-copy": button, "pre code": code, ".code-status": status };
  const listing = { querySelector: (selector) => elements[selector] };
  const document = {
    addEventListener: (_, handler) => handler(),
    querySelectorAll: () => [listing],
  };
  runInNewContext(script, { document, navigator: { clipboard } });
  return { button, code, status, click: () => events.click() };
}

test("copy controls stay hidden when the clipboard API is unavailable", () => {
  assert.equal(setup(undefined).button.hidden, true);
});

test("copy preserves source whitespace and excludes surrounding UI", async () => {
  let copied;
  const ui = setup({ writeText: async (text) => { copied = text; } });
  assert.equal(ui.button.hidden, false);
  await ui.click();
  assert.equal(copied, ui.code.textContent);
  assert.equal(ui.status.textContent, "Code copied.");
  assert.equal(ui.button.disabled, false);
});

test("clipboard rejection is announced and permits another attempt", async () => {
  const ui = setup({ writeText: async () => { throw new Error("Permission denied"); } });
  await ui.click();
  assert.match(ui.status.textContent, /Select the code/);
  assert.equal(ui.button.disabled, false);
});
