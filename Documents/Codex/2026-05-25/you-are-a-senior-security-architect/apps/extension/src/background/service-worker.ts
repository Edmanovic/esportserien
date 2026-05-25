type ExtensionHandshake = {
  type: "handshake";
  extension_origin: string;
  platform: "chromium-mv3" | "firefox-web-extension";
  extension_version: string;
  extension_nonce: number[];
  protocol_version: 1;
};

const HOST_NAME = "com.espass.desktop";

export async function connectNativeHost(): Promise<chrome.runtime.Port> {
  const port = chrome.runtime.connectNative(HOST_NAME);
  const handshake: ExtensionHandshake = {
    type: "handshake",
    extension_origin: chrome.runtime.getURL(""),
    platform: "chromium-mv3",
    extension_version: chrome.runtime.getManifest().version,
    extension_nonce: Array.from(crypto.getRandomValues(new Uint8Array(32))),
    protocol_version: 1,
  };
  port.postMessage(handshake);
  return port;
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message?.type !== "credential-request") {
    return false;
  }

  const senderOrigin = sender.url ? new URL(sender.url).origin : "";
  if (senderOrigin !== message.origin) {
    sendResponse({ ok: false, code: "sender-origin-mismatch" });
    return true;
  }

  connectNativeHost()
    .then((port) => {
      port.postMessage({
        type: "credential-request",
        origin: message.origin,
        top_level_origin: message.topLevelOrigin,
        user_gesture: message.userGesture === true,
      });
      sendResponse({ ok: true });
    })
    .catch(() => sendResponse({ ok: false, code: "native-host-unavailable" }));

  return true;
});

