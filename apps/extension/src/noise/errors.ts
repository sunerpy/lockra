/** Why a handshake or a channel failed: the connection cannot go on (the app's `NoiseError`). */
export type NoiseFailure =
  /** A message that does not decrypt: altered, replayed, or under other keys. */
  | "decrypt"
  /** A message too short to hold what the pattern says it holds. */
  | "short"
  /** An application message over 512 KiB or eight fragments. */
  | "too_long"
  /** A fragment without its flag byte, or with another flag. */
  | "malformed"
  /** A step out of order, or a key of the wrong size. */
  | "state";

export class NoiseError extends Error {
  constructor(readonly failure: NoiseFailure) {
    super(`noise: ${failure}`);
    this.name = "NoiseError";
  }
}
