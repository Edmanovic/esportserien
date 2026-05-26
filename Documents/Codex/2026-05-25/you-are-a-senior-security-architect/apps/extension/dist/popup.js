// apps/extension/src/popup/popup.ts
function renderUnavailable(root) {
  root.innerHTML = "<p>ESPASS k\xF8rer ikke. Start ESPASS-appen for at forts\xE6tte.</p>";
}
function renderLocked(root) {
  root.innerHTML = `
    <h3>ESPASS</h3>
    <label for="pw">Adgangskode</label>
    <input type="password" id="pw" placeholder="Masterkodeord" autofocus />
    <button id="unlock-btn">L\xE5s op</button>
    <div id="error"></div>
  `;
  const btn = document.getElementById("unlock-btn");
  const pwInput = document.getElementById("pw");
  const errorDiv = document.getElementById("error");
  btn.addEventListener("click", async () => {
    const password = pwInput.value;
    if (!password) {
      errorDiv.textContent = "Indtast adgangskode";
      return;
    }
    btn.disabled = true;
    btn.textContent = "L\xE5ser op...";
    const response = await chrome.runtime.sendMessage({ type: "unlock", password });
    if (response?.type === "ok" || response?.vault_state === "unlocked") {
      await main();
    } else {
      errorDiv.textContent = "Forkert adgangskode";
      btn.disabled = false;
      btn.textContent = "L\xE5s op";
      pwInput.focus();
    }
  });
}
function renderReady(root, autolockMinutes) {
  const autolockText = autolockMinutes != null ? `<p>Auto-lock: ${autolockMinutes} min</p>` : `<p>Auto-lock: aldrig</p>`;
  root.innerHTML = `
    <h3>ESPASS klar</h3>
    ${autolockText}
    <button id="lock-btn">L\xE5s</button>
  `;
  const lockBtn = document.getElementById("lock-btn");
  lockBtn.addEventListener("click", async () => {
    await chrome.runtime.sendMessage({ type: "lock" });
    await main();
  });
}
async function main() {
  const root = document.getElementById("root");
  const response = await chrome.runtime.sendMessage({ type: "get_vault_status" });
  switch (response?.state) {
    case "ready":
      renderReady(root, response.autolock_minutes ?? null);
      break;
    case "locked":
      renderLocked(root);
      break;
    default:
      renderUnavailable(root);
      break;
  }
}
main();
