// The import preview's choices, shared by the desktop and the phone: what may be done with each
// found account, and the choices a commit sends.
import type { CandidateAction, CandidateStatus, Choice, ImportView } from "./schema";

/** What the user may do with a found account or login: a new one is added or skipped; one that
 *  shares a name with an account of another secret, or a site and user name with a login that
 *  differs, can also replace it (a login's default); the rest are only skipped. */
export function actionsFor(status: CandidateStatus): readonly CandidateAction[] {
  if (status.type === "new") return ["add", "skip"];
  if (status.type === "conflict" || status.type === "login_conflict")
    return ["add", "replace", "skip"];
  return [];
}

/** The choice for every account the user can decide on: theirs, else the preview's default. */
export function importChoices(
  view: ImportView,
  chosen: ReadonlyMap<number, CandidateAction>,
): Choice[] {
  return view.candidates
    .filter((c) => actionsFor(c.status).length > 0)
    .map((c) => ({ id: c.id, action: chosen.get(c.id) ?? c.default_action }));
}

/** How many accounts and logins the commit takes in (added or replacing one). */
export function takenCount(choices: readonly Choice[]): number {
  return choices.filter((c) => c.action !== "skip").length;
}
