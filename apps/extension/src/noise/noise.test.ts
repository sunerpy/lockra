// @vitest-environment node
// The extension's Noise against the handshakes snow made with fixed keys (tests/vectors/noise.json):
// the initiator's messages byte for byte, the responder's read, the same hashes, the same code and
// the same channel messages.
import { type NoiseVectors, fixedKeyPair, hex, unhex, vector } from "../test/vectors";
import { utf8 } from "./bytes";
import { CipherState, Channel, FRAGMENT, MAX_FRAGMENTS, MAX_MESSAGE, MAX_NOISE } from "./channel";
import {
  Initiator,
  NoiseError,
  PAIRING_CODE_INFO,
  PAIRING_PATTERN,
  PROLOGUE,
  SESSION_PATTERN,
} from "./handshake";
import { pairingCode } from "./code";

const v = vector("noise.json") as NoiseVectors;

/** A channel message as sent: the flag byte, then the JSON. */
function withoutFlag(plainHex: string): string {
  return plainHex.slice(2);
}

describe("the constants", () => {
  it("are the Rust side's", () => {
    expect(new TextDecoder().decode(PROLOGUE)).toBe(v.prologue);
    expect(new TextDecoder().decode(PAIRING_CODE_INFO)).toBe(v.pairing_code_info);
    expect(PAIRING_PATTERN).toBe(v.pairing.pattern);
    expect(SESSION_PATTERN).toBe(v.session.pattern);
    expect(MAX_NOISE).toBe(65_535);
    expect(FRAGMENT).toBe(65_518);
    expect(MAX_FRAGMENTS).toBe(8);
    expect(MAX_MESSAGE).toBe(512 * 1024);
  });
});

describe("the pairing code", () => {
  it("is HKDF of the hash, modulo a million, with leading zeros", async () => {
    expect(await pairingCode(new Uint8Array(32))).toBe(v.pairing_code_of_zero_hash);
    expect(await pairingCode(unhex(v.pairing.handshake_hash))).toBe(v.pairing.code);
  });
});

describe("pairing (XX), the extension initiating", () => {
  it("writes snow's messages, reads the app's, and agrees on the hash, the code and the channel", async () => {
    const local = await fixedKeyPair(v.extension_static.private);
    expect(hex(local.publicKey)).toBe(v.extension_static.public);
    const handshake = await Initiator.pairing({
      local,
      ephemeral: () => fixedKeyPair(v.pairing.initiator_ephemeral),
    });
    expect(handshake.finished).toBe(false);
    expect(hex(await handshake.write())).toBe(v.pairing.messages[0]);
    expect(handshake.remoteStatic).toBeNull();
    const payload = await handshake.read(unhex(v.pairing.messages[1]));
    expect(payload).toHaveLength(0);
    expect(hex(handshake.remoteStatic ?? new Uint8Array())).toBe(v.app_static.public);
    expect(hex(await handshake.write())).toBe(v.pairing.messages[2]);
    expect(handshake.finished).toBe(true);
    expect(hex(handshake.hash)).toBe(v.pairing.handshake_hash);
    expect(await handshake.code()).toBe(v.pairing.code);

    const channel = await handshake.channel();
    const pushed = await channel.open(unhex(v.pairing.responder_first_message));
    expect(hex(pushed ?? new Uint8Array())).toBe(
      withoutFlag(v.pairing.responder_first_message_plain),
    );
    const request = unhex(withoutFlag(v.pairing.initiator_first_message_plain));
    expect((await channel.seal(request)).map(hex)).toEqual([v.pairing.initiator_first_message]);
  });

  it("fails on a second message that was altered", async () => {
    const handshake = await Initiator.pairing({
      local: await fixedKeyPair(v.extension_static.private),
      ephemeral: () => fixedKeyPair(v.pairing.initiator_ephemeral),
    });
    await handshake.write();
    const altered = unhex(v.pairing.messages[1]);
    altered[40] = (altered[40] ?? 0) ^ 1;
    await expect(handshake.read(altered)).rejects.toBeInstanceOf(NoiseError);
    await expect(handshake.read(unhex(v.pairing.messages[1]).slice(0, 50))).rejects.toThrow(
      "noise: state",
    );
  });

  it("refuses steps out of order", async () => {
    const handshake = await Initiator.pairing({
      local: await fixedKeyPair(v.extension_static.private),
    });
    await expect(handshake.read(unhex(v.pairing.messages[1]))).rejects.toBeInstanceOf(NoiseError);
    await expect(handshake.channel()).rejects.toBeInstanceOf(NoiseError);
    await expect(handshake.code()).rejects.toBeInstanceOf(NoiseError);
    // A real ephemeral key, made by WebCrypto: the first message is its public key.
    expect(await handshake.write()).toHaveLength(32);
    await expect(handshake.write()).rejects.toBeInstanceOf(NoiseError);
  });
});

describe("a session (KK) between the paired keys", () => {
  it("writes snow's first message, reads the app's, and agrees on the hash and the channel", async () => {
    const handshake = await Initiator.session({
      local: await fixedKeyPair(v.extension_static.private),
      remote: unhex(v.app_static.public),
      ephemeral: () => fixedKeyPair(v.session.initiator_ephemeral),
    });
    expect(hex(await handshake.write())).toBe(v.session.messages[0]);
    expect(await handshake.read(unhex(v.session.messages[1]))).toHaveLength(0);
    expect(handshake.finished).toBe(true);
    expect(hex(handshake.hash)).toBe(v.session.handshake_hash);
    expect(hex(handshake.remoteStatic ?? new Uint8Array())).toBe(v.app_static.public);

    const channel = await handshake.channel();
    const pushed = await channel.open(unhex(v.session.responder_first_message));
    expect(hex(pushed ?? new Uint8Array())).toBe(
      withoutFlag(v.session.responder_first_message_plain),
    );
    const request = unhex(withoutFlag(v.session.initiator_messages_plain));
    expect(hex((await channel.seal(request))[0] ?? new Uint8Array())).toBe(
      v.session.initiator_messages[0],
    );
    // The second message under the next nonce.
    expect(hex((await channel.seal(request))[0] ?? new Uint8Array())).toBe(
      v.session.initiator_messages[1],
    );
  });

  it("cannot read the app's answer under another idea of the app's key", async () => {
    const stranger = await fixedKeyPair(v.session.responder_ephemeral);
    const handshake = await Initiator.session({
      local: await fixedKeyPair(v.extension_static.private),
      remote: stranger.publicKey,
      ephemeral: () => fixedKeyPair(v.session.initiator_ephemeral),
    });
    expect(hex(await handshake.write())).not.toBe(v.session.messages[0]);
    await expect(handshake.read(unhex(v.session.messages[1]))).rejects.toBeInstanceOf(NoiseError);
  });

  it("needs a 32-byte key for the app", async () => {
    await expect(
      Initiator.session({
        local: await fixedKeyPair(v.extension_static.private),
        remote: new Uint8Array(31),
      }),
    ).rejects.toBeInstanceOf(NoiseError);
  });
});

/** Two ends of a channel under the same pair of keys, as a finished handshake would give. */
async function pair(): Promise<[Channel, Channel]> {
  const a = crypto.getRandomValues(new Uint8Array(32));
  const b = crypto.getRandomValues(new Uint8Array(32));
  // Each cipher wipes the key it was given: every side gets its own copy.
  return [
    new Channel(await CipherState.of(a.slice()), await CipherState.of(b.slice())),
    new Channel(await CipherState.of(b.slice()), await CipherState.of(a.slice())),
  ];
}

describe("the channel", () => {
  it("sends a long message in fragments, at most 65 535 bytes each, and puts it together", async () => {
    const [extension, app] = await pair();
    const long = new Uint8Array(FRAGMENT * 3 + 5).fill(120);
    const sealed = await extension.seal(long);
    expect(sealed).toHaveLength(4);
    expect(sealed.every((s) => s.length <= MAX_NOISE)).toBe(true);
    let got: Uint8Array | null = null;
    for (const fragment of sealed) got = await app.open(fragment);
    expect(got).toEqual(long);
    // The empty message is one fragment.
    const empty = await extension.seal(new Uint8Array());
    expect(empty).toHaveLength(1);
    expect(await app.open(empty[0] ?? new Uint8Array())).toEqual(new Uint8Array());
    expect(
      utf8.decode(
        (await app.open((await extension.seal(utf8.encode("{}")))[0] ?? new Uint8Array())) ??
          new Uint8Array(),
      ),
    ).toBe("{}");
  });

  it("refuses a message over eight fragments or 512 KiB", async () => {
    const [extension] = await pair();
    expect(await extension.seal(new Uint8Array(MAX_FRAGMENTS * FRAGMENT))).toHaveLength(
      MAX_FRAGMENTS,
    );
    await expect(
      extension.seal(new Uint8Array(MAX_FRAGMENTS * FRAGMENT + 1)),
    ).rejects.toBeInstanceOf(NoiseError);
    await expect(extension.seal(new Uint8Array(MAX_MESSAGE + 1))).rejects.toBeInstanceOf(
      NoiseError,
    );
  });

  it("ends on a ninth fragment, a flag other than 0 or 1, or a message that does not decrypt", async () => {
    const [extension, app] = await pair();
    const raw = await CipherState.of(new Uint8Array(32));
    const peer = new Channel(await CipherState.of(new Uint8Array(32)), raw);
    const sender = await CipherState.of(new Uint8Array(32));
    const opened: (Uint8Array | null)[] = [];
    for (let n = 0; n < MAX_FRAGMENTS; n++)
      opened.push(await peer.open(await sender.encrypt(new Uint8Array(0), Uint8Array.of(1, 99))));
    expect(opened).toEqual(Array.from({ length: MAX_FRAGMENTS }, () => null));
    await expect(
      peer.open(await sender.encrypt(new Uint8Array(0), Uint8Array.of(1, 99))),
    ).rejects.toBeInstanceOf(NoiseError);
    const flagged = await CipherState.of(new Uint8Array(32).fill(7));
    const reader = new Channel(
      await CipherState.of(new Uint8Array(32)),
      await CipherState.of(new Uint8Array(32).fill(7)),
    );
    await expect(
      reader.open(await flagged.encrypt(new Uint8Array(0), Uint8Array.of(7, 1))),
    ).rejects.toBeInstanceOf(NoiseError);
    const once = await extension.seal(utf8.encode("once"));
    expect(await app.open(once[0] ?? new Uint8Array())).toEqual(utf8.encode("once"));
    await expect(app.open(once[0] ?? new Uint8Array())).rejects.toBeInstanceOf(NoiseError);
    await expect(app.open(new Uint8Array(3))).rejects.toBeInstanceOf(NoiseError);
  });
});
