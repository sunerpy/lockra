// Screen-capture protection follows the secret views: Rust turns it on with each answer that shows
// a secret, and `secret_view_closed` turns it off. Several views of logins can be open at once (the
// detail pane's notes under the password's dialog, the generator over the form), so the webview
// counts them and says so only when the last one goes.
import { useBackend } from "@lockra/ui";
import { useEffect } from "react";

let open = 0;

/** While `active`, a secret is on the screen; the secret view ends with the last of them. */
export function useSecretView(active: boolean): void {
  const { backend } = useBackend();
  useEffect(() => {
    if (!active) return undefined;
    open += 1;
    return () => {
      open -= 1;
      if (open === 0)
        void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    };
  }, [active, backend]);
}
