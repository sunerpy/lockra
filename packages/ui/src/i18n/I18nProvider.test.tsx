import { render, screen } from "@testing-library/react";
import { Dialog } from "../components/Dialog";
import { TitleBar } from "../components/TitleBar";
import { I18nProvider, useI18n, useLocale, useT } from "./I18nProvider";

function Probe() {
  const t = useT();
  const i18n = useI18n();
  return (
    <span data-testid="probe" data-locale={useLocale()} data-tag={i18n.tag}>
      {t("shell.nav.codes")} · {t("common.accounts", { n: 1 })}
    </span>
  );
}

describe("I18nProvider", () => {
  it("defaults to zh-CN without a provider and follows the provider's locale", () => {
    const { unmount } = render(<Probe />);
    expect(screen.getByTestId("probe")).toHaveTextContent("验证码 · 1 个账号");
    expect(screen.getByTestId("probe").dataset.locale).toBe("zh-CN");
    unmount();
    render(
      <I18nProvider locale="en">
        <Probe />
      </I18nProvider>,
    );
    expect(screen.getByTestId("probe")).toHaveTextContent("Codes · 1 account");
    expect(screen.getByTestId("probe").dataset.tag).toBe("en-US");
  });

  it("mirrors the locale onto <html lang> only when asked", () => {
    document.documentElement.lang = "xx";
    const { unmount } = render(
      <I18nProvider locale="en">
        <Probe />
      </I18nProvider>,
    );
    expect(document.documentElement.lang).toBe("xx");
    unmount();
    render(
      <I18nProvider locale="en" documentLang>
        <Probe />
      </I18nProvider>,
    );
    expect(document.documentElement.lang).toBe("en-US");
  });

  it("shared components render English copy under an English provider", () => {
    render(
      <I18nProvider locale="en">
        <TitleBar
          title="Codes"
          onSearch={() => undefined}
          platform="windows"
          controls={{
            minimize: () => undefined,
            toggleMaximize: () => undefined,
            close: () => undefined,
          }}
        />
        <Dialog
          open
          title="Delete"
          onClose={() => undefined}
          actions={<button type="button">OK</button>}>
          body
        </Dialog>
      </I18nProvider>,
    );
    expect(screen.getByRole("button", { name: "Minimize" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Search or run a command · Ctrl K" }),
    ).toBeInTheDocument();
  });
});
