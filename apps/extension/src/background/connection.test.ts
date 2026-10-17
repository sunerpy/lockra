// @vitest-environment node
// The background's connection to Lockra on a fake native port: the hello and the versions, pairing
// with the code, sessions, locking and unlocking, revoking, the host's `not_running`, a missing host.
import { FakeApp } from "../test/fake-app";
import { ManualClock } from "../test/clock";
import { generateKeyPair, type KeyPair } from "../noise/crypto";
import { Connection, type Keys, type PairingRecord } from "./connection";
import type { ClientInfo } from "../protocol/schemas";

class MemoryKeys implements Keys {
  pair: KeyPair | null = null;
  record: PairingRecord | null = null;
  async staticPair(): Promise<KeyPair> {
    this.pair ??= await generateKeyPair();
    return this.pair;
  }
  async pairing(): Promise<PairingRecord | null> {
    return this.record;
  }
  async setPairing(record: PairingRecord | null): Promise<void> {
    this.record = record;
  }
}

const CLIENT: ClientInfo = {
  browser: "chrome",
  label: "",
  extension: "abcdefghijklmnopabcdefghijklmnop",
  version: "0.10.0",
  page_access: false,
};

async function setup(prepare?: (app: FakeApp) => void | Promise<void>) {
  const app = await FakeApp.start();
  await prepare?.(app);
  const keys = new MemoryKeys();
  const clock = new ManualClock();
  let client = CLIENT;
  const connection = new Connection({
    connectNative: app.connect,
    keys,
    clock,
    client: async () => client,
  });
  const locked = vi.fn();
  connection.onLocked(locked);
  return {
    app,
    keys,
    clock,
    connection,
    locked,
    setClient: (next: Partial<ClientInfo>) => {
      client = { ...client, ...next };
    },
  };
}

/** Pair through the fake app's approval; the session is open afterwards. */
async function paired(context: Awaited<ReturnType<typeof setup>>): Promise<string> {
  const { app, connection } = context;
  connection.start();
  await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
  await connection.pair();
  await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
  const id = await app.approve();
  await vi.waitFor(() => expect(connection.view.session).toBe("open"));
  return id;
}

describe("the hello", () => {
  it("says what the extension is and reads Lockra's state", async () => {
    const { app, connection } = await setup();
    const views: string[] = [];
    connection.subscribe((view) => views.push(view.link));
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    expect(app.posted[0]?.plain).toEqual({ op: "hello", proto_min: 1, proto_max: 1, ...CLIENT });
    expect(connection.view.app).toEqual({ state: "unlocked", version: "0.10.0" });
    expect(connection.view.paired).toBe(false);
    expect(connection.view.session).toBe("none");
    expect(views).toContain("connecting");
    // A second start keeps the one connection.
    connection.start();
    expect(app.ports).toHaveLength(1);
  });

  it("tells which side needs an update when the versions share none", async () => {
    const older = await setup((fake) => {
      fake.protoMin = 0;
      fake.protoMax = 0;
      fake.version = "0.9.0";
    });
    older.connection.start();
    await vi.waitFor(() => expect(older.connection.view.link).toBe("incompatible"));
    expect(older.connection.view.incompatible).toBe("app");
    expect(older.connection.view.app?.version).toBe("0.9.0");

    const newer = await setup((fake) => {
      fake.protoMin = 2;
      fake.protoMax = 3;
      fake.version = "0.12.0";
    });
    newer.connection.start();
    await vi.waitFor(() => expect(newer.connection.view.link).toBe("incompatible"));
    expect(newer.connection.view.incompatible).toBe("extension");
    // No handshake is tried.
    await expect(newer.connection.pair()).rejects.toThrow("Lockra is not ready to pair");
    expect(newer.app.posted.filter((m) => m.t === "hs")).toHaveLength(0);
  });

  it("is sent again when the extension's site access changes, which ends the session", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection, setClient } = context;
    const hellos = () => app.posted.filter((m) => m.plain?.op === "hello");
    const before = hellos().length;
    setClient({ page_access: true });
    connection.refreshHello();
    await vi.waitFor(() => expect(hellos()).toHaveLength(before + 1));
    expect(app.posted.findLast((m) => m.plain?.op === "hello")?.plain?.page_access).toBe(true);
    // A new session follows, with the same pairing.
    await vi.waitFor(() => expect(app.posted.filter((m) => m.t === "hs" && m.p)).toHaveLength(2));
    await vi.waitFor(() => expect(connection.view.session).toBe("open"));
  });
});

describe("pairing", () => {
  it("shows the app's code, keeps the pairing once approved and opens a session", async () => {
    const context = await setup();
    const { app, connection, keys } = context;
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    await connection.pair();
    expect(connection.view.pairing.stage).toBe("handshake");
    await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
    const pairing = connection.view.pairing;
    expect(pairing.stage === "waiting" && pairing.code).toBe(app.code);
    await vi.waitFor(() => {
      const view = connection.view.pairing;
      expect(view.stage === "waiting" && view.expiresAt).toBe(context.clock.now() + 120_000);
    });
    const id = await app.approve();
    await vi.waitFor(() => expect(connection.view.session).toBe("open"));
    expect(connection.view.pairing.stage).toBe("none");
    expect(connection.view.paired).toBe(true);
    expect(keys.record?.id).toBe(id);
    expect(keys.record?.app_public).toBe(Buffer.from(app.key.publicKey).toString("base64"));
    // The session's first message names the pairing.
    expect(app.posted.findLast((m) => m.t === "hs")?.p).toBe(id);
  });

  it("reports a refusal, and can be tried again", async () => {
    const { app, connection } = await setup();
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    await connection.pair();
    await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
    await app.deny();
    await vi.waitFor(() =>
      expect(connection.view.pairing).toEqual({ stage: "failed", reason: "denied" }),
    );
    expect(connection.view.paired).toBe(false);
    await connection.pair();
    await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
  });

  it("ends when the hello goes again (the site access changed)", async () => {
    const { connection } = await setup();
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    await connection.pair();
    await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
    connection.refreshHello();
    expect(connection.view.pairing).toEqual({ stage: "failed", reason: "ended" });
  });

  it("can be cancelled: the connection starts over", async () => {
    const { app, connection } = await setup();
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    await connection.pair();
    await vi.waitFor(() => expect(connection.view.pairing.stage).toBe("waiting"));
    const hellos = () => app.posted.filter((m) => m.plain?.op === "hello");
    const before = hellos().length;
    connection.cancelPairing();
    expect(connection.view.pairing.stage).toBe("none");
    await vi.waitFor(() => expect(hellos()).toHaveLength(before + 1));
    await expect(app.approve()).rejects.toThrow("nothing to approve");
  });

  it("needs Lockra unlocked; a locked app says so", async () => {
    const { app, connection } = await setup((fake) => {
      fake.state = "locked";
    });
    connection.start();
    await vi.waitFor(() => expect(connection.view.app?.state).toBe("locked"));
    await expect(connection.pair()).rejects.toThrow("Lockra is not ready to pair");
    // An app that locks between the hello and the handshake answers with an error.
    await app.unlock();
    await vi.waitFor(() => expect(connection.view.app?.state).toBe("unlocked"));
    app.state = "locked";
    await connection.pair();
    await vi.waitFor(() =>
      expect(connection.view.pairing).toEqual({ stage: "failed", reason: "locked" }),
    );
  });
});

describe("a session", () => {
  it("answers requests by number and checks them against the op's answer", async () => {
    const context = await setup();
    const id = await paired(context);
    const { app, connection } = context;
    const status = await connection.request({ op: "status" });
    expect(status).toEqual({
      ok: true,
      value: {
        pairing: id,
        app_version: "0.10.0",
        policy: {
          fill_otp: true,
          copy_otp: true,
          auto_submit: false,
          confirm_fill: false,
          cross_origin_frames: "never",
          on_page_load: false,
          // Not in this answer (an older Lockra's): off.
          match_port: false,
        },
        items: 3,
        paused: false,
      },
    });
    app.handlers.search = () => ({ error: "paused" });
    expect(await connection.request({ op: "search", text: "x", top_url: "" })).toEqual({
      ok: false,
      error: "paused",
    });
    app.handlers.otp = () => ({ code: 123456 });
    expect(
      await connection.request({ op: "otp", item_id: id, frame_url: "", top_url: "" }),
    ).toEqual({
      ok: false,
      error: "invalid_answer",
    });
    // Two requests at once: each gets its own answer.
    app.handlers.otp = (request) => ({ code: String(request.req), copied: false });
    const [a, b] = await Promise.all([
      connection.request({ op: "otp", item_id: id, frame_url: "", top_url: "" }),
      connection.request({ op: "otp", item_id: id, frame_url: "", top_url: "" }),
    ]);
    expect(a.ok && b.ok && a.value.code !== b.value.code).toBe(true);
    expect(app.requests.map((r) => r.op)).toEqual(["status", "search", "otp", "otp", "otp"]);
  });

  it("sends a long request in fragments", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection } = context;
    app.handlers.save = (request) => ({
      item_id: String(request.username).slice(0, 0) || crypto.randomUUID(),
    });
    const long = "x".repeat(200_000);
    const saved = await connection.request({
      op: "save",
      frame_url: "https://example.com/",
      top_url: "https://example.com/",
      title: "",
      username: long,
      password: "p",
      group: null,
    });
    expect(saved.ok).toBe(true);
    expect(app.requests.at(-1)?.username).toBe(long);
    expect(app.posted.filter((m) => m.t === "msg").length).toBeGreaterThan(3);
  });

  it("gives up on an answer that does not come", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection, clock } = context;
    app.handlers.status = () => new Promise(() => undefined);
    const waiting = connection.request({ op: "status" });
    await vi.waitFor(() => expect(app.requests).toHaveLength(1));
    clock.advance(15_000);
    expect(await waiting).toEqual({ ok: false, error: "timeout" });
  });

  it("is refused before it opens: no pairing, locked, or not running", async () => {
    const { connection } = await setup();
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    expect(await connection.request({ op: "status" })).toEqual({ ok: false, error: "not_paired" });
  });

  it("ends as Lockra locks and opens again as it unlocks", async () => {
    const context = await setup();
    const id = await paired(context);
    const { app, connection, locked } = context;
    app.handlers.status = () => new Promise(() => undefined);
    const waiting = connection.request({ op: "status" });
    await vi.waitFor(() => expect(app.requests).toHaveLength(1));
    await app.lock();
    await vi.waitFor(() => expect(connection.view.app?.state).toBe("locked"));
    expect(connection.view.session).toBe("none");
    expect(await waiting).toEqual({ ok: false, error: "ended" });
    expect(locked).toHaveBeenCalled();
    expect(await connection.request({ op: "status" })).toEqual({ ok: false, error: "locked" });
    await app.unlock();
    await vi.waitFor(() => expect(connection.view.session).toBe("open"));
    expect(app.posted.filter((m) => m.t === "hs" && m.p === id)).toHaveLength(2);
  });

  it("forgets the pairing when Lockra revokes it, or no longer knows it", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection, keys, locked } = context;
    await app.revoke();
    await vi.waitFor(() => expect(connection.view.paired).toBe(false));
    expect(connection.view.session).toBe("none");
    expect(keys.record).toBeNull();
    expect(locked).toHaveBeenCalled();

    // A pairing removed while the extension was away: the session's handshake gets `not_paired`.
    const other = await setup();
    await paired(other);
    other.app.peers.clear();
    other.connection.refreshHello();
    await vi.waitFor(() => expect(other.connection.view.paired).toBe(false));
    expect(other.keys.record).toBeNull();
  });

  it("is forgotten on the extension's side when the user unpairs it there", async () => {
    const context = await setup();
    await paired(context);
    const { connection, keys } = context;
    await connection.forget();
    expect(connection.view.paired).toBe(false);
    expect(connection.view.session).toBe("none");
    expect(keys.record).toBeNull();
  });

  it("ignores pushes of a later version", async () => {
    const context = await setup();
    await paired(context);
    await context.app.push({ push: "later_push", n: 1 });
    expect((await context.connection.request({ op: "status" })).ok).toBe(true);
  });
});

describe("Lockra not running, or no host", () => {
  it("shows not running, starts Lockra and says hello every second until it answers", async () => {
    const { app, connection, clock } = await setup((fake) => {
      fake.running = false;
    });
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("not_running"));
    connection.startApp(false);
    await vi.waitFor(() => expect(app.posted.some((m) => m.plain?.op === "start_app")).toBe(true));
    expect(app.posted.find((m) => m.plain?.op === "start_app")?.plain).toEqual({ op: "start_app" });
    expect(connection.view.starting).toBe(true);
    const hellos = () => app.posted.filter((m) => m.plain?.op === "hello").length;
    clock.advance(1_000);
    await vi.waitFor(() => expect(hellos()).toBe(2));
    app.running = true;
    clock.advance(1_000);
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    expect(connection.view.starting).toBe(false);
    // The polling stops once it answered.
    clock.advance(10_000);
    expect(hellos()).toBe(3);
  });

  it("stops polling after thirty seconds", async () => {
    const { app, connection, clock } = await setup((fake) => {
      fake.running = false;
    });
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("not_running"));
    connection.startApp(true);
    await vi.waitFor(() =>
      expect(app.posted.find((m) => m.plain?.op === "start_app")?.plain).toEqual({
        op: "start_app",
        show: true,
      }),
    );
    for (let second = 0; second < 40; second++) {
      clock.advance(1_000);
      await app.idle();
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    const hellos = app.posted.filter((m) => m.plain?.op === "hello").length;
    expect(hellos).toBeGreaterThan(20);
    expect(hellos).toBeLessThanOrEqual(32);
    expect(connection.view.starting).toBe(false);
  });

  it("hears Lockra quit, and asks again when the popup or a page needs it", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection, locked } = context;
    await app.quit();
    await vi.waitFor(() => expect(connection.view.link).toBe("not_running"));
    expect(connection.view.session).toBe("none");
    expect(locked).toHaveBeenCalled();
    app.running = true;
    connection.ensure();
    await vi.waitFor(() => expect(connection.view.session).toBe("open"));
  });

  it("lets pages try a missing host once a minute and a stopped Lockra every five seconds", async () => {
    const { app, connection, clock } = await setup((fake) => {
      fake.noHost = "Specified native messaging host not found.";
    });
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("no_host"));
    connection.ensure(true);
    expect(app.ports).toHaveLength(1);
    clock.advance(60_000);
    connection.ensure(true);
    expect(app.ports).toHaveLength(2);
    // That host is missing too: its port closes a moment later.
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(connection.view.link).toBe("no_host");
    app.noHost = null;
    app.running = false;
    connection.ensure();
    await vi.waitFor(() => expect(connection.view.link).toBe("not_running"));
    const hellos = () => app.posted.filter((m) => m.plain?.op === "hello").length;
    const before = hellos();
    connection.ensure(true);
    await app.idle();
    expect(hellos()).toBe(before);
    clock.advance(5_000);
    connection.ensure(true);
    await vi.waitFor(() => expect(hellos()).toBe(before + 1));
  });

  it("shows no host when the port closes before a word, and tries again on demand", async () => {
    const { app, connection } = await setup((fake) => {
      fake.noHost = "Specified native messaging host not found.";
    });
    connection.start();
    await vi.waitFor(() => expect(connection.view.link).toBe("no_host"));
    expect(connection.view.detail).toBe("Specified native messaging host not found.");
    app.noHost = null;
    connection.ensure();
    await vi.waitFor(() => expect(connection.view.link).toBe("ready"));
    expect(app.ports).toHaveLength(2);
  });

  it("reconnects with a growing pause when a host that spoke goes away", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection, clock } = context;
    app.port?.close("host exited");
    await vi.waitFor(() => expect(connection.view.link).toBe("connecting"));
    expect(connection.view.session).toBe("none");
    expect(app.ports).toHaveLength(1);
    clock.advance(1_000);
    expect(app.ports).toHaveLength(2);
    await vi.waitFor(() => expect(connection.view.session).toBe("open"));
  });

  it("treats an envelope it cannot read as a broken connection", async () => {
    const context = await setup();
    await paired(context);
    const { app, connection, clock } = context;
    app.port?.deliver({ v: 9, t: "msg", d: "" });
    await vi.waitFor(() => expect(connection.view.link).toBe("connecting"));
    expect(app.port?.closed).toBe(true);
    clock.advance(1_000);
    await vi.waitFor(() => expect(connection.view.session).toBe("open"));
  });
});

describe("unlocking from the extension", () => {
  it("asks Lockra, which says whether it passed it on", async () => {
    const { app, connection, clock } = await setup((fake) => {
      fake.state = "locked";
    });
    connection.start();
    await vi.waitFor(() => expect(connection.view.app?.state).toBe("locked"));
    expect(await connection.unlock()).toBe(true);
    expect(app.posted.some((m) => m.plain?.op === "unlock_request")).toBe(true);
    app.state = "unlocked";
    expect(await connection.unlock()).toBe(false);
    // No answer at all (the host is there, the app is not): false after a while.
    app.running = false;
    const answer = connection.unlock();
    await vi.waitFor(() =>
      expect(app.posted.filter((m) => m.plain?.op === "unlock_request")).toHaveLength(3),
    );
    clock.advance(5_000);
    expect(await answer).toBe(false);
  });
});
