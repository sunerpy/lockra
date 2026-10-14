// The keys the extension keeps (plan §一 信任边界 2): its static X25519 key pair, made once by
// WebCrypto with a private key that cannot be exported and kept as a CryptoKey in IndexedDB, and
// the pairing (its id and the app's public key) in storage.local. Nothing else is kept: no login,
// no secret, no session key.
import type { StorageArea } from "../browser/api";
import { type KeyPair, generateKeyPair } from "../noise/crypto";
import { fromBase64 } from "../noise/bytes";
import type { Keys, PairingRecord } from "./connection";

/** Where the static key pair lives. */
export interface KeyVault {
  load(): Promise<KeyPair | null>;
  save(pair: KeyPair): Promise<void>;
}

const PAIRING = "pairing";

export class KeyStore implements Keys {
  private pair: Promise<KeyPair> | null = null;

  constructor(
    private readonly vault: KeyVault,
    private readonly local: StorageArea,
  ) {}

  /** The extension's static key pair, made the first time it is needed. */
  staticPair(): Promise<KeyPair> {
    this.pair ??= (async () => {
      const kept = await this.vault.load();
      if (kept) return kept;
      const made = await generateKeyPair();
      await this.vault.save(made);
      return made;
    })().catch((error: unknown) => {
      this.pair = null;
      throw error;
    });
    return this.pair;
  }

  async pairing(): Promise<PairingRecord | null> {
    const stored = (await this.local.get(PAIRING))[PAIRING];
    return isPairing(stored) ? stored : null;
  }

  async setPairing(record: PairingRecord | null): Promise<void> {
    if (record) await this.local.set({ [PAIRING]: record });
    else await this.local.remove(PAIRING);
  }
}

function isPairing(value: unknown): value is PairingRecord {
  return (
    typeof value === "object" &&
    value !== null &&
    "id" in value &&
    typeof value.id === "string" &&
    "app_public" in value &&
    typeof value.app_public === "string" &&
    fromBase64(value.app_public)?.length === 32 &&
    "paired_at" in value &&
    typeof value.paired_at === "number"
  );
}

const DB = "lockra";
const STORE = "keys";
const KEY = "static";

function request<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.addEventListener("success", () => resolve(req.result));
    req.addEventListener("error", () => reject(req.error ?? new Error("IndexedDB failed")));
  });
}

function open(): Promise<IDBDatabase> {
  const req = indexedDB.open(DB, 1);
  req.addEventListener("upgradeneeded", () => req.result.createObjectStore(STORE));
  return request(req);
}

/** The key pair in IndexedDB: a CryptoKey is stored as itself, still not extractable. */
export const indexedDbVault: KeyVault = {
  async load() {
    const db = await open();
    try {
      const stored: unknown = await request(db.transaction(STORE).objectStore(STORE).get(KEY));
      if (
        typeof stored !== "object" ||
        stored === null ||
        !("privateKey" in stored) ||
        !("publicKey" in stored)
      )
        return null;
      const { privateKey, publicKey } = stored;
      if (
        !(privateKey instanceof CryptoKey) ||
        !(publicKey instanceof Uint8Array) ||
        publicKey.length !== 32
      )
        return null;
      return { privateKey, publicKey };
    } finally {
      db.close();
    }
  },
  async save(pair) {
    const db = await open();
    try {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).put({ privateKey: pair.privateKey, publicKey: pair.publicKey }, KEY);
      await new Promise<void>((resolve, reject) => {
        tx.addEventListener("complete", () => resolve());
        tx.addEventListener("error", () => reject(tx.error ?? new Error("IndexedDB failed")));
      });
    } finally {
      db.close();
    }
  },
};
