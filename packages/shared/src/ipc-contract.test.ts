// The Rust side writes these fixtures (lockra-bridge tests/contract.rs); every one must parse with
// the schemas here, the command list must match the bridge's, and replaying a command through
// TauriBackend must send exactly the fixture's payload.
import { ipcFixtures } from "./fixtures";
import {
  COMMAND_NAMES,
  RESULT_SCHEMAS,
  PHONE_COMMAND_NAMES,
  SHELL_COMMAND_NAMES,
  codesFrameSchema,
  coreErrorSchema,
  entryAddedSchema,
  exportPageSchema,
  exportStartedSchema,
  importOutcomeSchema,
  revealedSchema,
  syncCreatedSchema,
  syncInviteSchema,
  syncViewSchema,
  uiCommandSchema,
  uiEventSchema,
  uiStateSchema,
  updateViewSchema,
} from "./schema";
import { TauriBackend, type TauriTransport } from "./tauri-backend";

function recordingTransport(answer: unknown = null): {
  transport: TauriTransport;
  calls: [string, Record<string, unknown> | undefined][];
} {
  const calls: [string, Record<string, unknown> | undefined][] = [];
  const transport: TauriTransport = {
    invoke: async (command, args) => {
      calls.push([command, args]);
      return answer;
    },
    listen: async () => () => undefined,
    channel: () => ({ onmessage: () => undefined }),
  };
  return { transport, calls };
}

describe("IPC fixtures", () => {
  it("every state parses", () => {
    const failures = Object.entries(ipcFixtures.state).filter(
      ([, state]) => !uiStateSchema.safeParse(state).success,
    );
    expect(failures.map(([name]) => name)).toEqual([]);
    const candidates = uiStateSchema.parse(ipcFixtures.state.unlocked).import?.candidates;
    expect(candidates).toHaveLength(8);
    expect(candidates?.map((c) => c.source.type)).toEqual(
      expect.arrayContaining(["file", "text", "clipboard", "camera"]),
    );
  });

  it("every event parses and every notice kind appears", () => {
    const kinds = new Set<string>();
    for (const event of ipcFixtures.events) {
      const parsed = uiEventSchema.parse(event);
      if (parsed.type === "notice") kinds.add(parsed.notice.type);
    }
    expect(kinds.size).toBe(11);
  });

  it("every update view parses, each state and each install method once at least", () => {
    const views = ipcFixtures.update.map((view) => updateViewSchema.parse(view));
    expect(new Set(views.map((v) => v.status.state))).toEqual(
      new Set([
        "idle",
        "checking",
        "up_to_date",
        "available",
        "downloading",
        "ready",
        "installing",
        "failed",
      ]),
    );
    expect(new Set(views.map((v) => v.method))).toEqual(
      new Set(["deb", "rpm", "appimage", "nsis", "msi", "app", null]),
    );
    expect(uiStateSchema.parse(ipcFixtures.state.unlocked).update.status.state).toBe("available");
  });

  it("every sync view parses, each state and both storages once at least", () => {
    const views = ipcFixtures.sync.map((view) => syncViewSchema.parse(view));
    const spaces = views.flatMap((v) => (v.space ? [v.space] : []));
    expect(new Set(spaces.map((s) => s.status.state))).toEqual(
      new Set(["idle", "syncing", "synced", "failed"]),
    );
    expect(new Set(spaces.map((s) => s.storage.kind))).toEqual(new Set(["s3", "webdav"]));
    expect(views.some((v) => v.space === null)).toBe(true);
    const unlocked = uiStateSchema.parse(ipcFixtures.state.unlocked).sync.space;
    expect(unlocked?.devices.map((d) => d.this_device)).toEqual([true, false]);
    expect(uiStateSchema.parse(ipcFixtures.state.locked).sync.space).toBeNull();
    // The storage's secret never comes back.
    expect(JSON.stringify(ipcFixtures.sync)).not.toMatch(/secret_access_key|"password"/);
  });

  it("every answer parses with its schema", () => {
    const r = ipcFixtures.responses;
    expect(entryAddedSchema.parse(r.entry_added).id).toMatch(/[0-9a-f-]{36}/);
    expect(exportStartedSchema.parse(r.export_started).excluded[0]?.reason).toBe("period_not_30");
    expect(exportPageSchema.parse(r.export_page).entry_ids).toHaveLength(2);
    expect(revealedSchema.parse(r.revealed).secret).toBe("JBSW Y3DP EHPK 3PXP");
    expect(importOutcomeSchema.parse(r.import_outcome)).toEqual({
      added: 3,
      replaced: 1,
      skipped: 2,
    });
    expect(codesFrameSchema.parse(r.codes_frame).codes[1]?.next_code).toBeNull();
    expect(codesFrameSchema.parse(r.codes_frame_locked).codes).toEqual([]);
    expect(syncCreatedSchema.parse(r.sync_created).sync_key).toMatch(/^LKS1-/);
    expect(syncInviteSchema.parse(r.sync_invite).invite).toMatch(/^lockra-invite:1:/);
    const errors = r.errors.map((e) => coreErrorSchema.parse(e));
    expect(errors[1]).toEqual({ code: "rate_limited", retry_at_ms: 1_790_000_004_000 });
  });

  it("the command list is the bridge's, in its order", () => {
    const names = ipcFixtures.commands.commands.map((c) => c.command);
    expect(names).toEqual(COMMAND_NAMES);
    expect(ipcFixtures.commands.shell_commands).toEqual([...SHELL_COMMAND_NAMES]);
    expect(ipcFixtures.commands.phone_commands).toEqual([...PHONE_COMMAND_NAMES]);
    const failures = ipcFixtures.commands.commands.filter(
      (command) => !uiCommandSchema.safeParse(command).success,
    );
    expect(failures.map((c) => c.command)).toEqual([]);
  });

  it("replaying a command sends exactly its fixture through lockra_dispatch", async () => {
    for (const fixture of ipcFixtures.commands.commands) {
      const command = uiCommandSchema.parse(fixture);
      const answers: Record<string, unknown> = {
        app_state: ipcFixtures.state.locked,
        entry_add_uri: ipcFixtures.responses.entry_added,
        entry_add_manual: ipcFixtures.responses.entry_added,
        entry_reveal: ipcFixtures.responses.revealed,
        import_commit: ipcFixtures.responses.import_outcome,
        export_start: ipcFixtures.responses.export_started,
        export_page: ipcFixtures.responses.export_page,
        sync_create: ipcFixtures.responses.sync_created,
        sync_invite: ipcFixtures.responses.sync_invite,
      };
      const { transport, calls } = recordingTransport(answers[command.command] ?? null);
      const answer = await new TauriBackend(transport).dispatch(command);
      expect(calls).toEqual([["lockra_dispatch", { command: fixture }]]);
      expect({ command: command.command, answered: answer !== null }).toEqual({
        command: command.command,
        answered: command.command in RESULT_SCHEMAS,
      });
    }
  });
});
