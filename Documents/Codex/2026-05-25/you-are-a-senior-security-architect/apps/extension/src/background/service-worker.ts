/**
 * ESPASS background service worker (MV3).
 *
 * - One persistent native-messaging port shared across all tabs.
 * - Response routing by request_id (random UUID added to every outgoing message).
 * - Credential cache: Map<origin, Credential[]> — cleared on vault_locked.
 * - Content script long-lived ports: used to push vault_locked events.
 * - Popup messages: handled via chrome.runtime.onMessage (sendMessage).
 */

const HOST_NAME = "com.espass.desktop";

interface Credential {
  id: string;
  title: string;
  username: string;
}

interface PendingRequest {
  resolve: (value: Record<string, unknown>) => void;
  timeoutId: ReturnType<typeof setTimeout>;
}

// ---------------------------------------------------------------------------
// Native host connection
// ---------------------------------------------------------------------------

let nativePort: chrome.runtime.Port | null = null;
const pendingRequests = new Map<string, PendingRequest>();
const credentialCache = new Map<string, Credential[]>(); // origin → items

function getOrConnectNativeHost(): chrome.runtime.Port {
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

function sendToNativeHost(
  msg: Record<string, unknown>
): Promise<Record<string, unknown>> {
  return new Promise((resolve) => {
    const requestId = crypto.randomUUID();
    msg.request_id = requestId;

    const timeoutId = setTimeout(() => {
      pendingRequests.delete(requestId);
      resolve({ type: "error", code: "timeout" });
    }, 10_000);

    pendingRequests.set(requestId, { resolve, timeoutId });
    getOrConnectNativeHost().postMessage(msg);
  });
}

function handleNativeMessage(msg: unknown): void {
  if (!msg || typeof msg !== "object") return;
  const m = msg as Record<string, unknown>;

  if (m.type === "vault_locked") {
    credentialCache.clear();
    broadcastToContentPorts({ type: "vault_locked" });
    return;
  }

  const requestId = m.request_id as string | undefined;
  if (!requestId) return;

  const pending = pendingRequests.get(requestId);
  if (!pending) return;

  clearTimeout(pending.timeoutId);
  pendingRequests.delete(requestId);
  pending.resolve(m);
}

// ---------------------------------------------------------------------------
// Content script long-lived ports
// ---------------------------------------------------------------------------

const contentPorts: chrome.runtime.Port[] = [];

function broadcastToContentPorts(msg: unknown): void {
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
    async (msg: Record<string, unknown>) => {
      if (!msg || typeof msg.type !== "string") return;
      const requestId = msg.request_id as string | undefined;
      let response: Record<string, unknown>;

      switch (msg.type) {
        case "find_credentials": {
          const origin = msg.origin as string;
          const cached = credentialCache.get(origin);
          if (cached) {
            response = { type: "credentials", items: cached };
          } else {
            response = await sendToNativeHost({ type: "find_credentials", origin });
            if (response.type === "credentials") {
              credentialCache.set(origin, response.items as Credential[]);
            }
          }
          break;
        }
        case "fill_credential": {
          const raw = await sendToNativeHost({
            type: "get_credential",
            id: msg.id as string,
          });
          if (raw.type === "credential") {
            response = {
              type: "fill_data",
              username: raw.username,
              password: raw.password,
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

// ---------------------------------------------------------------------------
// Popup messages (sendMessage — not long-lived ports)
// ---------------------------------------------------------------------------

chrome.runtime.onMessage.addListener(
  (
    message: Record<string, unknown>,
    _sender: chrome.runtime.MessageSender,
    sendResponse: (response: unknown) => void
  ) => {
    if (!message || typeof message.type !== "string") return false;

    switch (message.type) {
      case "get_vault_status": {
        resolveVaultStatus().then(sendResponse);
        return true;
      }
      case "unlock": {
        sendToNativeHost({
          type: "unlock",
          password: message.password as string,
        }).then(sendResponse);
        return true;
      }
      case "lock": {
        sendToNativeHost({ type: "lock" }).then((raw) => {
          credentialCache.clear();
          sendResponse(raw);
        });
        return true;
      }
      default:
        return false;
    }
  }
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function resolveVaultStatus(): Promise<Record<string, unknown>> {
  try {
    const raw = await sendToNativeHost({ type: "status" });
    if (raw.type === "status") {
      const state = raw.vault_state === "unlocked" ? "ready" : "locked";
      return {
        type: "vault_status",
        state,
        autolock_minutes: raw.autolock_minutes ?? null,
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
