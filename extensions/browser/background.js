// Windows AI browser extension (Chrome & Edge, MV3).
// Content is only sent when the user explicitly picks a menu item or
// popup button; nothing is collected in the background.
import { HOST, SELECTION_ACTIONS, extractPageInPage, friendlyError, isRestrictedUrl, pageCapture, selectionCapture } from "./lib.js";

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "wai-selection", title: "Windows AI", contexts: ["selection"] });
    for (const a of SELECTION_ACTIONS) {
      chrome.contextMenus.create({ id: `wai-sel-${a.id}`, parentId: "wai-selection", title: a.title, contexts: ["selection"] });
    }
    chrome.contextMenus.create({ id: "wai-page-summarize", title: "Summarize this page with Windows AI", contexts: ["page"] });
    chrome.contextMenus.create({ id: "wai-page-ask", title: "Ask Windows AI about this page…", contexts: ["page"] });
  });
});

export async function send(payload) {
  const resp = await chrome.runtime.sendNativeMessage(HOST, { type: "capture", payload });
  if (!resp || !resp.ok) throw new Error(resp?.error || "Windows AI did not accept the content");
  return resp;
}

async function flash(tabId, ok, text) {
  try {
    await chrome.action.setBadgeBackgroundColor({ tabId, color: ok ? "#8b5cf6" : "#ef4444" });
    await chrome.action.setBadgeText({ tabId, text: ok ? "✓" : "!" });
    await chrome.action.setTitle({ tabId, title: text });
    setTimeout(() => chrome.action.setBadgeText({ tabId, text: "" }), 4000);
  } catch {
    /* tab may be gone */
  }
}

async function capturePage(tab, action, question) {
  if (isRestrictedUrl(tab.url)) throw new Error("This page can't be read by extensions.");
  const [{ result }] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: extractPageInPage });
  return send(pageCapture({ ...result, action, question }));
}

export async function handle(menuId, info, tab) {
  try {
    if (menuId.startsWith("wai-sel-")) {
      const action = menuId.slice("wai-sel-".length);
      await send(selectionCapture({ text: info.selectionText, title: tab?.title, url: info.pageUrl || tab?.url, action }));
    } else if (menuId === "wai-page-summarize") {
      await capturePage(tab, "summarize");
    } else if (menuId === "wai-page-ask") {
      await capturePage(tab, null);
    } else {
      return;
    }
    flash(tab?.id, true, "Sent to Windows AI");
  } catch (e) {
    flash(tab?.id, false, friendlyError(e?.message));
    console.warn("Windows AI:", e);
  }
}

chrome.contextMenus.onClicked.addListener((info, tab) => handle(String(info.menuItemId), info, tab));

chrome.commands.onCommand.addListener(async (command, tab) => {
  if (command !== "ask-selection" || !tab || isRestrictedUrl(tab.url)) return;
  try {
    const [{ result }] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: () => String(getSelection() || "") });
    if (!result.trim()) return flash(tab.id, false, "Select some text first");
    await send(selectionCapture({ text: result, title: tab.title, url: tab.url, action: "ask" }));
    flash(tab.id, true, "Sent to Windows AI");
  } catch (e) {
    flash(tab.id, false, friendlyError(e?.message));
  }
});

// Popup requests.
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  (async () => {
    try {
      if (msg.type === "ping") return reply(await chrome.runtime.sendNativeMessage(HOST, { type: "ping" }));
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (msg.type === "page") return reply(await capturePage(tab, msg.action, msg.question));
      reply({ ok: false, error: "unknown request" });
    } catch (e) {
      reply({ ok: false, error: friendlyError(e?.message) });
    }
  })();
  return true;
});
