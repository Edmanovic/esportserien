function renderUnavailable(root: HTMLElement): void {
  root.innerHTML =
    "<p>ESPASS kører ikke. Start ESPASS-appen for at fortsætte.</p>";
}

function renderLocked(root: HTMLElement): void {
  root.innerHTML = `
    <h3>ESPASS</h3>
    <label for="pw">Adgangskode</label>
    <input type="password" id="pw" placeholder="Masterkodeord" autofocus />
    <button id="unlock-btn">Lås op</button>
    <div id="error"></div>
  `;

  const btn = document.getElementById("unlock-btn") as HTMLButtonElement;
  const pwInput = document.getElementById("pw") as HTMLInputElement;
  const errorDiv = document.getElementById("error") as HTMLDivElement;

  btn.addEventListener("click", async () => {
    const password = pwInput.value;
    if (!password) {
      errorDiv.textContent = "Indtast adgangskode";
      return;
    }

    btn.disabled = true;
    btn.textContent = "Låser op...";

    const response = await chrome.runtime.sendMessage({ type: "unlock", password }) as Record<string, unknown> | undefined;

    if (response?.type === "ok" || response?.vault_state === "unlocked") {
      await main();
    } else {
      errorDiv.textContent = "Forkert adgangskode";
      btn.disabled = false;
      btn.textContent = "Lås op";
      pwInput.focus();
    }
  });
}

function renderReady(root: HTMLElement, autolockMinutes: number | null): void {
  const autolockText =
    autolockMinutes != null
      ? `<p>Auto-lock: ${autolockMinutes} min</p>`
      : `<p>Auto-lock: aldrig</p>`;

  root.innerHTML = `
    <h3>ESPASS klar</h3>
    ${autolockText}
    <button id="lock-btn">Lås</button>
  `;

  const lockBtn = document.getElementById("lock-btn") as HTMLButtonElement;
  lockBtn.addEventListener("click", async () => {
    await chrome.runtime.sendMessage({ type: "lock" });
    await main();
  });
}

async function main(): Promise<void> {
  const root = document.getElementById("root") as HTMLElement;
  const response = await chrome.runtime.sendMessage({ type: "get_vault_status" }) as Record<string, unknown> | undefined;

  switch (response?.state) {
    case "ready":
      renderReady(root, (response.autolock_minutes as number | null | undefined) ?? null);
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
