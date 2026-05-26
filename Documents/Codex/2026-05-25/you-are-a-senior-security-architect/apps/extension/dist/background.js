// apps/extension/src/background/service-worker.ts
var HOST_NAME = "com.espass.desktop";
var nativePort = null;
var pendingRequests = /* @__PURE__ */ new Map();
var credentialCache = /* @__PURE__ */ new Map();
function getOrConnectNativeHost() {
  if (nativePort) return nativePort;
  nativePort = chrome.runtime.connectNative(HOST_NAME);
  nativePort.onMessage.addListener(handleNativeMessage);
  nativePort.onDisconnect.addListener(() => {
    nativePort = null;
    for (const [id, pending] of pendingRequests) {
      clearTimeout(pending.timeoutId);
      pending.resolve({ type: "error", code: "native-host-disconnected" });
      pendingRequests.delete(id);
    }
    broadcastToContentPorts({ type: "vault_status", state: "unavailable" });
  });
  return nativePort;
}
function sendToNativeHost(msg) {
  return new Promise((resolve) => {
    const requestId = crypto.randomUUID();
    msg.request_id = requestId;
    const timeoutId = setTimeout(() => {
      pendingRequests.delete(requestId);
      resolve({ type: "error", code: "timeout" });
    }, 1e4);
    pendingRequests.set(requestId, { resolve, timeoutId });
    getOrConnectNativeHost().postMessage(msg);
  });
}
function handleNativeMessage(msg) {
  if (!msg || typeof msg !== "object") return;
  const m = msg;
  if (m.type === "vault_locked") {
    credentialCache.clear();
    broadcastToContentPorts({ type: "vault_locked" });
    return;
  }
  const requestId = m.request_id;
  if (!requestId) return;
  const pending = pendingRequests.get(requestId);
  if (!pending) return;
  clearTimeout(pending.timeoutId);
  pendingRequests.delete(requestId);
  pending.resolve(m);
}
var contentPorts = [];
function broadcastToContentPorts(msg) {
  for (let i = contentPorts.length - 1; i >= 0; i--) {
    try {
      contentPorts[i].postMessage(msg);
    } catch {
      contentPorts.splice(i, 1);
    }
  }
}
chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "espass-content") return;
  contentPorts.push(port);
  port.onDisconnect.addListener(() => {
    const idx = contentPorts.indexOf(port);
    if (idx !== -1) contentPorts.splice(idx, 1);
  });
  port.onMessage.addListener(
    async (msg) => {
      if (!msg || typeof msg.type !== "string") return;
      const requestId = msg.request_id;
      let response;
      switch (msg.type) {
        case "find_credentials": {
          const origin = msg.origin;
          const cached = credentialCache.get(origin);
          if (cached) {
            response = { type: "credentials", items: cached };
          } else {
            response = await sendToNativeHost({ type: "find_credentials", origin });
            if (response.type === "credentials") {
              credentialCache.set(origin, response.items);
            }
          }
          break;
        }
        case "fill_credential": {
          const raw = await sendToNativeHost({
            type: "get_credential",
            id: msg.id
          });
          if (raw.type === "credential") {
            response = {
              type: "fill_data",
              username: raw.username,
              password: raw.password
            };
          } else {
            response = raw;
          }
          break;
        }
        case "get_vault_status": {
          response = await resolveVaultStatus();
          break;
        }
        default:
          return;
      }
      if (requestId) {
        port.postMessage({ ...response, request_id: requestId });
      }
    }
  );
});
chrome.runtime.onMessage.addListener(
  (message, _sender, sendResponse) => {
    if (!message || typeof message.type !== "string") return false;
    switch (message.type) {
      case "get_vault_status": {
        resolveVaultStatus().then(sendResponse);
        return true;
      }
      case "unlock": {
        sendToNativeHost({
          type: "unlock",
          password: message.password
        }).then(sendResponse);
        return true;
      }
      case "lock": {
        credentialCache.clear();
        sendToNativeHost({ type: "lock" }).then(sendResponse);
        return true;
      }
      default:
        return false;
    }
  }
);
async function resolveVaultStatus() {
  try {
    const raw = await sendToNativeHost({ type: "status" });
    if (raw.type === "status") {
      const state = raw.vault_state === "unlocked" ? "ready" : "locked";
      return {
        type: "vault_status",
        state,
        autolock_minutes: raw.autolock_minutes ?? null
      };
    }
    if (raw.code === "native-host-disconnected") {
      return { type: "vault_status", state: "unavailable" };
    }
    return { type: "vault_status", state: "locked" };
  } catch {
    return { type: "vault_status", state: "unavailable" };
  }
}
