// The background: a service worker in Chromium, an event page in Firefox. It holds the connection to
// Lockra (the session's keys in its memory only) and answers the extension's pages and content
// scripts. When the browser stops it, everything it held goes; the next event starts it again.
import { browserApi } from "../browser/api";
import { brandsOf, clientInfo } from "./client-info";
import { systemClock } from "./clock";
import { Connection } from "./connection";
import { Controller } from "./controller";
import { KeyStore, indexedDbVault } from "./keys";
import { nativePort } from "./native";

const api = browserApi();
const brands = brandsOf(navigator);
const connection = new Connection({
  connectNative: () => nativePort(api),
  keys: new KeyStore(indexedDbVault, api.storage.local),
  clock: systemClock,
  client: () => clientInfo(api, brands),
});
new Controller({
  api,
  connection,
  clock: systemClock,
  extensionVersion: api.runtime.getManifest().version,
}).install();
connection.start();
