const $ = (id) => document.getElementById(id);
const status = (t) => ($("status").textContent = t);

chrome.runtime.sendMessage({ type: "ping" }, (r) => {
  const ok = r && r.ok;
  $("dot").className = `dot ${ok ? "ok" : "err"}`;
  if (!ok) status(r?.error || "Windows AI is not reachable.");
  else if (r.enabled === false) status("Integrations are turned off in Windows AI settings.");
});

function sendPage(action, question) {
  status("Sending…");
  chrome.runtime.sendMessage({ type: "page", action, question }, (r) => {
    if (r && r.ok) {
      status("Sent — check the Windows AI window.");
      setTimeout(() => window.close(), 900);
    } else status(r?.error || "Failed");
  });
}

$("summarize").onclick = () => sendPage("summarize", null);
$("ask").onclick = () => {
  const q = $("question").value.trim();
  sendPage(null, q || null);
};
