export type LockState = "locked" | "unlocked";

export type DesktopCredentialRecord = {
  id: string;
  origin: string;
  username: string;
  password: string;
  updatedAt: string;
};

export class DesktopVaultController {
  #state: LockState = "locked";
  #records = new Map<string, DesktopCredentialRecord>();

  state(): LockState {
    return this.#state;
  }

  markUnlocked(): void {
    this.#state = "unlocked";
  }

  lock(): void {
    this.#state = "locked";
    this.#records.clear();
  }

  upsertCredential(record: DesktopCredentialRecord): void {
    this.#requireUnlocked();
    this.#records.set(record.id, { ...record });
  }

  findByOrigin(origin: string): DesktopCredentialRecord[] {
    this.#requireUnlocked();
    return [...this.#records.values()]
      .filter((record) => record.origin === origin)
      .map((record) => ({ ...record }));
  }

  #requireUnlocked(): void {
    if (this.#state !== "unlocked") {
      throw new Error("vault-locked");
    }
  }
}

export class SecureClipboardRuntime {
  #clearTimer: ReturnType<typeof setTimeout> | undefined;

  async copySecret(value: string, ttlMs = 30_000): Promise<void> {
    if (ttlMs > 60_000) {
      throw new Error("clipboard-ttl-too-long");
    }
    await navigator.clipboard.writeText(value);
    if (this.#clearTimer) {
      clearTimeout(this.#clearTimer);
    }
    this.#clearTimer = setTimeout(() => {
      void navigator.clipboard.writeText("");
    }, ttlMs);
  }
}

export class IdleLockRuntime {
  #lastActivity = Date.now();
  #timer: ReturnType<typeof setInterval> | undefined;

  constructor(
    private readonly idleMs: number,
    private readonly onLock: () => void,
  ) {}

  start(): void {
    this.stop();
    this.#timer = setInterval(() => {
      if (Date.now() - this.#lastActivity >= this.idleMs) {
        this.onLock();
      }
    }, Math.min(this.idleMs, 5_000));
  }

  stop(): void {
    if (this.#timer) {
      clearInterval(this.#timer);
      this.#timer = undefined;
    }
  }

  touch(): void {
    this.#lastActivity = Date.now();
  }
}

