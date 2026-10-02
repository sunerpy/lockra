// The IPC fixtures written by lockra-bridge's contract test (`UPDATE_IPC_FIXTURES=1 cargo test -p
// lockra-bridge --test contract`), as raw JSON: the tests validate them with the schemas.
import commands from "./ipc/commands.json";
import events from "./ipc/events.json";
import responses from "./ipc/responses.json";
import state from "./ipc/state.json";
import sync from "./ipc/sync.json";
import update from "./ipc/update.json";

export const ipcFixtures = { commands, events, responses, state, sync, update } as const;
