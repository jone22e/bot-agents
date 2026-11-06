// A extensao roda no perfil Chromium em que o usuario a carregar. Somente
// comandos do host Native Messaging registrado para este ID chegam aqui.
const HOST = "com.c2s.dots";
let port = null;
let commandQueue = Promise.resolve();

function connect() {
  if (port) return;
  try {
    const next = chrome.runtime.connectNative(HOST);
    port = next;
    const reply = message => { try { next.postMessage(message); } catch { /* a desconexao sera tratada abaixo */ } };
    next.onMessage.addListener(message => {
      if (message?.kind !== "command") return;
      const task = commandQueue.then(() => handle(message));
      commandQueue = task.then(() => {}, () => {});
      void task.then(
        value => reply({ kind: "result", id: message.id, ok: true, value }),
        error => reply({ kind: "result", id: message.id, ok: false, error: String(error?.message || error) }),
      );
    });
    next.onDisconnect.addListener(() => {
      port = null;
      void chrome.runtime.lastError;
      setTimeout(connect, 5000);
    });
  } catch {
    port = null;
    setTimeout(connect, 5000);
  }
}

chrome.runtime.onInstalled.addListener(connect);
chrome.runtime.onStartup.addListener(connect);
chrome.alarms.create("dots-connect", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(connect);
connect();

async function tabFor(dotId) {
  const { dotsTabs = {} } = await chrome.storage.session.get("dotsTabs");
  if (dotsTabs[dotId] == null) return null;
  try { return await chrome.tabs.get(dotsTabs[dotId]); }
  catch { return null; }
}

function tabOrigin(url) {
  try { return new URL(url).origin; }
  catch { return null; }
}

function createdTabs(data) {
  return new Set([...(data.dotsCreatedTabIds || []), ...Object.entries(data.dotsTabOwned || {})
    .filter(([, owned]) => owned).map(([dotId]) => data.dotsTabs?.[dotId]).filter(id => id != null)]);
}

async function bindTab(dotId, tab, data) {
  const tabs = data.dotsTabs || {};
  const owned = data.dotsTabOwned || {};
  const created = createdTabs(data);
  const previousId = tabs[dotId];
  let previous = null;
  if (previousId != null && previousId !== tab.id) {
    try { previous = await chrome.tabs.get(previousId); } catch { /* ja fechada */ }
  }
  tabs[dotId] = tab.id;
  owned[dotId] = created.has(tab.id);
  // Abas criadas para outro site permanecem disponiveis para a proxima visita.
  // Uma duplicata criada pelo DOT pode ser fechada quando ninguem mais a usa.
  const duplicate = previous && created.has(previous.id)
    && (previous.url === "about:blank" || tabOrigin(previous.url) === tabOrigin(tab.url))
    && !Object.entries(tabs).some(([id, tabId]) => id !== dotId && tabId === previous.id);
  if (duplicate) created.delete(previous.id);
  await chrome.storage.session.set({ dotsTabs: tabs, dotsTabOwned: owned, dotsCreatedTabIds: [...created] });
  if (duplicate) {
    try { await chrome.tabs.remove(previous.id); } catch { /* ja fechada */ }
  }
  return tab;
}

async function newTab(focus, data) {
  const tab = await chrome.tabs.create({ url: "about:blank", active: focus });
  const created = createdTabs(data);
  created.add(tab.id);
  data.dotsCreatedTabIds = [...created];
  return tab;
}

async function siteTabFor(dotId, url, focus) {
  const data = await chrome.storage.session.get(["dotsTabs", "dotsTabOwned", "dotsCreatedTabIds", "dotsSiteTabs"]);
  const origin = tabOrigin(url);
  const candidates = (await chrome.tabs.query({})).filter(tab => tabOrigin(tab.url) === origin);
  const preferredId = (data.dotsSiteTabs || {})[origin];
  const current = candidates.find(tab => tab.id === data.dotsTabs?.[dotId]);
  const exact = candidates.filter(tab => tab.url === url);
  const created = createdTabs(data);
  const selected = (current && current.url !== url ? current : null)
    || exact.find(tab => tab.id === preferredId)
    || exact.find(tab => tab.active)
    || exact[0]
    || candidates.find(tab => tab.id === preferredId)
    || candidates.find(tab => created.has(tab.id))
    || candidates.find(tab => tab.active)
    || candidates[0]
    || await newTab(focus, data);
  data.dotsSiteTabs = { ...data.dotsSiteTabs, [origin]: selected.id };
  await chrome.storage.session.set({ dotsSiteTabs: data.dotsSiteTabs });
  return bindTab(dotId, selected, data);
}

const isWhatsApp = url => {
  try { return new URL(url).origin === "https://web.whatsapp.com"; }
  catch { return false; }
};

// Consulta apenas se a aba do WhatsApp esta pronta ou mostra o aviso de outra
// janela. Nenhum texto da conversa sai da pagina nesta selecao.
function whatsappTabState() {
  const labels = new Set([...document.querySelectorAll('button,[role="button"]')].map(e =>
    (e.getAttribute("aria-label") || e.getAttribute("title") || e.innerText || e.textContent || "").trim()));
  const text = document.body?.innerText || "";
  return {
    ready: labels.has("Conversas") && labels.has("Nova conversa"),
    blocked: text.includes("O WhatsApp está aberto em outra janela") || text.includes("WhatsApp is open in another window"),
  };
}

// O WhatsApp Web aceita uma sessao ativa por perfil. Reaproveita a aba ja
// aberta pelo usuario, sem tocar nas demais abas do perfil.
async function whatsappTabFor(dotId, focus) {
  const data = await chrome.storage.session.get(["dotsTabs", "dotsTabOwned", "dotsCreatedTabIds", "whatsappSharedTabId"]);
  const candidates = await chrome.tabs.query({ url: "https://web.whatsapp.com/*" });
  const inspected = await Promise.all(candidates.map(async tab => {
    try {
      const result = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: whatsappTabState });
      return { tab, ready: result[0]?.result?.ready === true, blocked: result[0]?.result?.blocked === true };
    } catch { return { tab, ready: false, blocked: false }; }
  }));
  const ready = inspected.filter(item => item.ready && !item.blocked);
  const usable = ready.length ? ready : inspected.filter(item => !item.blocked);
  let selected = usable.length === 1 ? usable[0] : null;
  if (!selected && usable.length > 1) {
    selected = usable.find(item => item.tab.id === data.whatsappSharedTabId)
      || (usable.filter(item => item.tab.active).length === 1 ? usable.find(item => item.tab.active) : null);
  }
  if (!selected && candidates.length) throw new Error("nao foi possivel identificar uma unica aba utilizavel do WhatsApp");
  const shared = selected?.tab;
  if (!shared) {
    const tab = await newTab(focus, data);
    await chrome.storage.session.set({ whatsappSharedTabId: tab.id });
    return bindTab(dotId, tab, data);
  }
  await chrome.storage.session.set({ whatsappSharedTabId: shared.id });
  return bindTab(dotId, shared, data);
}

async function waitLoaded(tabId) {
  for (let i = 0; i < 40; i++) {
    const tab = await chrome.tabs.get(tabId);
    if (tab.status === "complete") return;
    await new Promise(resolve => setTimeout(resolve, 250));
  }
}

// Executa em mundo isolado: refs da leitura nao sao alteraveis pelo site.
function pageOperation(op, args, dotId) {
  const visible = e => {
    const r = e.getBoundingClientRect(), s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
  };
  const role = e => e.getAttribute("role") || ({ A: "link", BUTTON: "button", INPUT: e.type === "checkbox" ? "checkbox" : "textbox", TEXTAREA: "textbox", SELECT: "combobox" }[e.tagName] || (e.isContentEditable ? "textbox" : e.tagName.toLowerCase()));
  const name = e => (e.getAttribute("aria-label") || e.getAttribute("title") || e.getAttribute("placeholder") || e.labels?.[0]?.innerText || e.innerText || e.textContent || "").trim().replace(/\s+/g, " ").slice(0, 120);
  if (op !== "read" && window.__dotsRefUrlByDot?.[dotId] !== location.href) throw new Error("pagina mudou; use dots_browser_read");
  if (op === "describe") {
    const e = window.__dotsRefsByDot?.[dotId]?.[args.ref - 1];
    if (!e?.isConnected) throw new Error("referencia vencida; use dots_browser_read");
    return `${role(e)} "${name(e)}"`;
  }
  if (op !== "read") {
    const e = window.__dotsRefsByDot?.[dotId]?.[args.ref - 1];
    if (op !== "press" && !e?.isConnected) throw new Error("referencia vencida; use dots_browser_read");
    if (op === "click") {
      e.scrollIntoView({ block: "center" });
      e.click();
    } else if (op === "type") {
      e.focus();
      if (e.isContentEditable) {
        if (args.clear) {
          const range = document.createRange();
          range.selectNodeContents(e);
          const selection = getSelection();
          selection.removeAllRanges();
          selection.addRange(range);
        }
        if (!document.execCommand("insertText", false, args.text)) throw new Error("campo nao aceitou o texto");
      } else if ("value" in e) {
        const proto = e.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
        if (!setter) throw new Error("elemento nao aceita texto");
        setter.call(e, (args.clear ? "" : e.value) + args.text);
        e.dispatchEvent(new Event("input", { bubbles: true }));
        e.dispatchEvent(new Event("change", { bubbles: true }));
      } else throw new Error("elemento nao aceita texto");
      if (args.submit) {
        e.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", bubbles: true }));
        e.form?.requestSubmit();
      }
    } else if (op === "select") {
      if (e.tagName !== "SELECT") throw new Error("elemento nao e lista de opcoes");
      const option = [...e.options].find(o => o.value === args.option || o.text === args.option);
      if (!option) throw new Error("opcao nao encontrada");
      e.value = option.value;
      e.dispatchEvent(new Event("input", { bubbles: true }));
      e.dispatchEvent(new Event("change", { bubbles: true }));
    } else if (op === "press") {
      if (!["Enter", "Escape"].includes(args.key)) throw new Error("tecla indisponivel no Chromium pessoal; use clique ou digitacao");
      const focused = document.activeElement;
      if (!focused) throw new Error("nenhum elemento em foco");
      focused.dispatchEvent(new KeyboardEvent("keydown", { key: args.key, code: args.key, bubbles: true }));
      if (args.key === "Enter") focused.form?.requestSubmit();
      if (args.key === "Escape") focused.blur();
    } else throw new Error(`operacao desconhecida: ${op}`);
    return { acted: true };
  }
  const usefulRoles = new Set(["button", "link", "textbox", "searchbox", "checkbox", "radio", "combobox", "option", "tab", "menuitem", "menuitemcheckbox", "menuitemradio", "row", "gridcell", "listitem", "treeitem", "switch"]);
  const list = [...document.querySelectorAll('a,button,input,textarea,select,[role],[contenteditable="true"]')].filter(e => visible(e) && (!e.hasAttribute("role") || usefulRoles.has(e.getAttribute("role")))).slice(0, 700);
  (window.__dotsRefsByDot ||= Object.create(null))[dotId] = list;
  (window.__dotsRefUrlByDot ||= Object.create(null))[dotId] = location.href;
  const elements = list.map((e, i) => {
    const value = e.type !== "password" && (e.tagName === "INPUT" || e.tagName === "TEXTAREA")
      ? String(e.value || "").trim().replace(/\s+/g, " ").slice(0, 120) : "";
    return `- ${role(e)} "${name(e).replace(/"/g, '\\"')}" [ref=${i + 1}]${value ? ` value="${value.replace(/"/g, '\\"')}"` : ""}`;
  }).join("\n");
  return {
    url: location.href,
    title: document.title,
    loaded: document.readyState === "complete",
    text: `Elementos:\n${elements}\n\nTexto da pagina:\n${(document.body?.innerText || document.body?.textContent || "").slice(0, 18000)}`,
  };
}

async function inject(tabId, op, args = {}, dotId) {
  const result = await chrome.scripting.executeScript({ target: { tabId }, func: pageOperation, args: [op, args, dotId] });
  if (!result.length) throw new Error("pagina nao respondeu");
  return result[0].result;
}

async function readAndClaim(tabId, dotId) {
  const page = await inject(tabId, "read", {}, dotId);
  const { dotsRefOwners = {} } = await chrome.storage.session.get("dotsRefOwners");
  dotsRefOwners[tabId] = { dotId, url: page.url };
  await chrome.storage.session.set({ dotsRefOwners });
  return page;
}

async function handle({ dot_id: dotId, op, args = {} }) {
  if (op === "open" && !/^https?:\/\//.test(args.url || "")) throw new Error("URL deve comecar com http ou https");
  let tab = op === "open" && isWhatsApp(args.url)
    ? await whatsappTabFor(dotId, !!args.focus)
    : op === "open" ? await siteTabFor(dotId, args.url, !!args.focus) : await tabFor(dotId);
  if (op === "status") return { running: !!tab, url: tab?.url || "", title: tab?.title || "" };
  if (!tab) throw new Error("nenhuma aba aberta para este DOT; use dots_browser_open");
  if (op === "open") {
    if (tab.url !== args.url) tab = await chrome.tabs.update(tab.id, { url: args.url });
    if (args.focus) await chrome.tabs.update(tab.id, { active: true });
    await waitLoaded(tab.id);
  }
  if (op !== "open" && op !== "read" && op !== "status") {
    const { dotsRefOwners = {} } = await chrome.storage.session.get("dotsRefOwners");
    if (dotsRefOwners[tab.id]?.dotId !== dotId) throw new Error("aba usada por outro DOT; use dots_browser_read antes de agir");
  }
  if (op === "describe") return inject(tab.id, op, args, dotId);
  if (op !== "open" && op !== "read") {
    await inject(tab.id, op, args, dotId);
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  return readAndClaim(tab.id, dotId);
}
