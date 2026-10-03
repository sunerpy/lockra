import { MockBackend, type MockOptions, sampleEntries } from "@lockra/shared/mock";
import { render, screen } from "@testing-library/react";
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

/** Wait for the first state: a screen is up. */
export async function ready(): Promise<void> {
  await screen.findAllByTestId(/^page-/);
}
