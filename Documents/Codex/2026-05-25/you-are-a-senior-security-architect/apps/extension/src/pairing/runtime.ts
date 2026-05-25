export type PairingState = "unpaired" | "pairing" | "paired";

export type ExtensionSession = {
  sessionId: string;
  expiresAt: string;
  counter: number;
};

export class BrowserPairingRuntime {
  #state: PairingState = "unpaired";
  #session: ExtensionSession | undefined;

  state(): PairingState {
    return this.#state;
  }

  beginPairing(): void {
    this.#state = "pairing";
    this.#session = undefined;
  }

  acceptSession(session: ExtensionSession): void {
    if (new Date(session.expiresAt).getTime() <= Date.now()) {
      throw new Error("stale-session");
    }
    this.#state = "paired";
    this.#session = { ...session };
  }

  nextCounter(): number {
    if (!this.#session) {
      throw new Error("extension-unpaired");
    }
    if (new Date(this.#session.expiresAt).getTime() <= Date.now()) {
      this.clear();
      throw new Error("session-expired");
    }
    this.#session.counter += 1;
    return this.#session.counter;
  }

  clear(): void {
    this.#state = "unpaired";
    this.#session = undefined;
  }
}

export class RuntimeOriginValidator {
  static exactMatch(origin: string, topLevelOrigin: string, savedOrigin: string): boolean {
    if (!origin.startsWith("https://") || !topLevelOrigin.startsWith("https://")) {
      return false;
    }
    return origin === topLevelOrigin && origin === savedOrigin;
  }
}

