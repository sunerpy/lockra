import { MockBackend, type MockOptions, sampleEntries } from "@lockra/shared/mock";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "../App";

export interface RenderAppOptions {
  backend?: MockBackend;
  mock?: MockOptions;
}

/** The whole app on an in-memory core: the sample vault, unlocked, in Chinese. */
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

/** Wait for the first state: a screen is up, and the effects of the render that brought it have
 *  run. The listeners a test fires events at (the lock on leaving the screen) are added in those
 *  effects, which the scheduler runs after the screen is in the page: on a busy machine, an event
 *  sent before them found no listener (CI, main at d5191e1). */
export async function ready(): Promise<void> {
  await screen.findAllByTestId(/^page-/);
  await act(async () => {});
}
