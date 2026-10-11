import { MockBackend, type MockOptions, sampleEntries } from "@lockra/shared/mock";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "../App";

export interface RenderAppOptions {
  backend?: MockBackend;
  mock?: MockOptions;
}

/** The whole app on an in-memory core: the sample vault, unlocked, in Chinese (jsdom's own
 *  `navigator.language` is `en-US`, which "follow the system" would turn into English). */
export function renderApp({ backend, mock }: RenderAppOptions = {}) {
  const core =
    backend ??
    new MockBackend({
      entries: sampleEntries(),
      ...mock,
      settings: { locale: "zh-cn", ...mock?.settings },
    });
  const user = userEvent.setup();
  const view = render(<App backend={core} />);
  return { ...view, backend: core, user };
}

/** Wait for the first state: a page is on screen (the shell's body is `page-body`). */
export async function ready(): Promise<void> {
  await screen.findAllByTestId(/^page-(?!body)/);
  // The effects of the render that brought the screen, where its listeners are added.
  await act(async () => {});
}
