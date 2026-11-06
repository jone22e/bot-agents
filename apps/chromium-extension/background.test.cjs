const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const vm = require("node:vm");
const test = require("node:test");
const assert = require("node:assert/strict");

const source = readFileSync(join(__dirname, "background.js"), "utf8");
const whatsapp = "https://web.whatsapp.com/";

function extension(initialTabs, initialStorage = {}) {
  const tabs = new Map(initialTabs.map(tab => [tab.id, { status: "complete", ...tab }]));
  const storage = { ...initialStorage };
  const calls = { created: [], updated: [], removed: [] };
  const listener = { addListener() {} };
  const chrome = {
    runtime: {
      connectNative: () => ({ onMessage: listener, onDisconnect: listener, postMessage() {} }),
      onInstalled: listener,
      onStartup: listener,
    },
    alarms: { create() {}, onAlarm: listener },
    storage: { session: {
      async get() { return structuredClone(storage); },
      async set(values) { Object.assign(storage, structuredClone(values)); },
    } },
    tabs: {
      async get(id) {
        if (!tabs.has(id)) throw new Error("tab closed");
        return { ...tabs.get(id) };
      },
      async query({ url }) {
        return [...tabs.values()].filter(tab => !url || (url === "https://web.whatsapp.com/*" && tab.url?.startsWith(whatsapp))).map(tab => ({ ...tab }));
      },
      async create(options) {
        const tab = { id: 100 + calls.created.length, url: options.url, active: options.active, status: "complete" };
        tabs.set(tab.id, tab);
        calls.created.push(options);
        return { ...tab };
      },
      async update(id, patch) {
        calls.updated.push({ id, patch });
        Object.assign(tabs.get(id), patch);
        return { ...tabs.get(id) };
      },
      async remove(id) { calls.removed.push(id); tabs.delete(id); },
    },
    scripting: { async executeScript({ target, func, args }) {
      const tab = tabs.get(target.tabId);
      if (func.name === "whatsappTabState") return [{ result: { ready: tab.ready || false, blocked: tab.blocked || false } }];
      if (args[0] === "describe") return [{ result: 'button "Enviar"' }];
      if (args[0] !== "read") return [{ result: { acted: true } }];
      return [{ result: { url: tab.url, title: "WhatsApp", loaded: true, text: "" } }];
    } },
  };
  const context = vm.createContext({ chrome, URL, setTimeout(callback) { callback(); } });
  vm.runInContext(source, context);
  return {
    storage,
    calls,
    tabs,
    open: (dotId, url = whatsapp) => vm.runInContext(`handle(${JSON.stringify({ dot_id: dotId, op: "open", args: { url } })})`, context),
    act: (dotId, op, args = {}) => vm.runInContext(`handle(${JSON.stringify({ dot_id: dotId, op, args })})`, context),
    pageWithInput: (value, type = "text") => {
      const input = {
        tagName: "INPUT", type, value, isConnected: true,
        getAttribute: key => key === "placeholder" ? "Pesquisar" : null,
        hasAttribute: () => false,
        getBoundingClientRect: () => ({ width: 100, height: 20 }),
      };
      context.window = {};
      context.location = { href: whatsapp };
      context.getComputedStyle = () => ({ visibility: "visible", display: "block" });
      context.document = { querySelectorAll: () => [input], body: { innerText: "" }, title: "WhatsApp", readyState: "complete" };
      return vm.runInContext('pageOperation("read", {}, "camila")', context);
    },
    probe: (labels, bodyText) => {
      context.document = {
        querySelectorAll: () => labels.map(label => ({ getAttribute: key => key === "title" ? label : null, innerText: "", textContent: "" })),
        body: { innerText: bodyText },
      };
      return vm.runInContext("whatsappTabState()", context);
    },
  };
}

test("detects the inbox and the duplicate-session notice without reading chats", () => {
  const app = extension([]);
  const inbox = app.probe(["Conversas", "Nova conversa"], "Mensagens");
  assert.equal(inbox.ready, true);
  assert.equal(inbox.blocked, false);
  const duplicate = app.probe([], "O WhatsApp está aberto em outra janela. Clique em Usar aqui.");
  assert.equal(duplicate.ready, false);
  assert.equal(duplicate.blocked, true);
});

test("shows the search input value in the snapshot without exposing passwords", () => {
  const app = extension([]);
  assert.match(app.pageWithInput("+55 47 9167-9549").text, /value="\+55 47 9167-9549"/);
  assert.doesNotMatch(app.pageWithInput("private-secret", "password").text, /private-secret/);
});

test("reuses the user's existing WhatsApp tab without navigating it", async () => {
  const app = extension([{ id: 10, url: whatsapp, active: true }]);
  await app.open("camila");
  assert.equal(app.storage.dotsTabs.camila, 10);
  assert.equal(app.storage.dotsTabOwned.camila, false);
  assert.equal(app.storage.whatsappSharedTabId, 10);
  assert.deepEqual(app.calls, { created: [], updated: [], removed: [] });
});

test("replaces a duplicate DOT tab with the existing WhatsApp tab", async () => {
  const app = extension([
    { id: 10, url: whatsapp, active: true },
    { id: 20, url: whatsapp, active: false },
  ], { dotsTabs: { camila: 20 }, dotsTabOwned: { camila: true } });
  await app.open("camila");
  assert.equal(app.storage.dotsTabs.camila, 10);
  assert.deepEqual(app.calls.removed, [20]);
  assert.ok(app.tabs.has(10));
});

test("chooses the inbox when neither WhatsApp tab is active", async () => {
  const app = extension([
    { id: 10, url: whatsapp, active: false, ready: true },
    { id: 20, url: whatsapp, active: false, blocked: true },
  ]);
  await app.open("camila");
  assert.equal(app.storage.dotsTabs.camila, 10);
  assert.equal(app.calls.created.length, 0);
});

test("refuses to guess when two inactive tabs are indistinguishable", async () => {
  const app = extension([
    { id: 10, url: whatsapp, active: false },
    { id: 20, url: whatsapp, active: false },
  ]);
  await assert.rejects(app.open("camila"), /nao foi possivel identificar/);
  assert.equal(app.calls.created.length, 0);
});

test("another DOT shares the same WhatsApp tab", async () => {
  const app = extension([{ id: 10, url: whatsapp, active: true }], {
    dotsTabs: { camila: 10 }, dotsTabOwned: { camila: false }, whatsappSharedTabId: 10,
  });
  await app.open("marina");
  assert.equal(app.storage.dotsTabs.marina, 10);
  assert.deepEqual(app.calls.created, []);
  assert.deepEqual(app.calls.removed, []);
});

test("the first DOT tab is shared when no WhatsApp tab exists yet", async () => {
  const app = extension([]);
  await app.open("camila");
  await app.open("marina");
  assert.equal(app.storage.dotsTabs.camila, app.storage.dotsTabs.marina);
  assert.equal(app.calls.created.length, 1);
});

test("other websites still use a DOT tab", async () => {
  const app = extension([{ id: 10, url: whatsapp, active: true }]);
  await app.open("camila", "https://example.com/");
  assert.equal(app.calls.created.length, 1);
  assert.equal(app.storage.dotsTabOwned.camila, true);
  assert.ok(app.tabs.has(10));
});

test("reuses an existing tab of another site across DOTs", async () => {
  const url = "https://app.example.com/orders";
  const app = extension([{ id: 10, url, active: true }]);
  await app.open("camila", url);
  await app.open("marina", url);
  assert.equal(app.storage.dotsTabs.camila, 10);
  assert.equal(app.storage.dotsTabs.marina, 10);
  assert.equal(app.storage.dotsTabOwned.camila, false);
  assert.equal(app.calls.created.length, 0);
  assert.equal(app.calls.updated.length, 0);
});

test("prefers the requested page already open over navigating another tab of the site", async () => {
  const app = extension([
    { id: 10, url: "https://app.example.com/orders" },
    { id: 20, url: "https://app.example.com/products" },
  ], { dotsSiteTabs: { "https://app.example.com": 10 } });
  await app.open("camila", "https://app.example.com/products");
  assert.equal(app.storage.dotsTabs.camila, 20);
  assert.equal(app.calls.updated.length, 0);
});

test("keeps the DOT on its current tab when moving within the same site", async () => {
  const app = extension([
    { id: 10, url: "https://app.example.com/orders" },
    { id: 20, url: "https://app.example.com/products" },
  ], {
    dotsTabs: { camila: 20 }, dotsTabOwned: { camila: false },
    dotsSiteTabs: { "https://app.example.com": 10 },
  });
  await app.open("camila", "https://app.example.com/orders");
  assert.equal(app.storage.dotsTabs.camila, 20);
  assert.equal(app.tabs.get(20).url, "https://app.example.com/orders");
  assert.ok(app.tabs.has(10));
});

test("reuses one tab per site while DOTs alternate pages and sites", async () => {
  const app = extension([]);
  await app.open("camila", "https://app.example.com/orders");
  await app.open("marina", "https://app.example.com/products");
  assert.equal(app.storage.dotsTabs.camila, app.storage.dotsTabs.marina);
  await app.open("camila", "https://mail.example.com/inbox");
  await app.open("marina", "https://app.example.com/orders");
  assert.equal(app.calls.created.length, 2);
  assert.equal(app.tabs.size, 2);
});

test("a DOT must read again after another DOT uses the shared tab", async () => {
  const url = "https://app.example.com/orders";
  const app = extension([{ id: 10, url }]);
  await app.open("camila", url);
  await app.open("marina", url);
  await assert.rejects(app.act("camila", "click", { ref: 1 }), /aba usada por outro DOT/);
  await app.act("camila", "read");
  await app.act("camila", "click", { ref: 1 });
});

test("closes only an obsolete DOT-created duplicate, never the user's tab", async () => {
  const url = "https://app.example.com/orders";
  const app = extension([{ id: 10, url, active: true }, { id: 20, url }], {
    dotsTabs: { camila: 20 }, dotsTabOwned: { camila: true },
  });
  await app.open("camila", url);
  assert.equal(app.storage.dotsTabs.camila, 10);
  assert.deepEqual(app.calls.removed, [20]);
  assert.ok(app.tabs.has(10));
});

test("leaving WhatsApp never navigates the shared user tab", async () => {
  const app = extension([{ id: 10, url: whatsapp, active: true }], {
    dotsTabs: { camila: 10 }, dotsTabOwned: { camila: false }, whatsappSharedTabId: 10,
  });
  await app.open("camila", "https://example.com/");
  assert.equal(app.tabs.get(10).url, whatsapp);
  assert.equal(app.storage.dotsTabOwned.camila, true);
  assert.notEqual(app.storage.dotsTabs.camila, 10);
});

test("leaving a DOT-created WhatsApp tab preserves it for another DOT", async () => {
  const app = extension([]);
  await app.open("camila");
  await app.open("marina");
  const sharedId = app.storage.whatsappSharedTabId;
  await app.open("camila", "https://example.com/");
  assert.equal(app.tabs.get(sharedId).url, whatsapp);
  assert.equal(app.storage.dotsTabs.marina, sharedId);
});
