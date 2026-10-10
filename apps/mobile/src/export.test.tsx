import { MOCK_PASSWORD } from "@lockra/shared/mock";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import { depthOf } from "./app/nav";
import { ready, renderApp } from "./test/render";

afterEach(async () => {
  cleanup();
  await waitFor(() => {
    if (depthOf(history.state) !== 0) throw new Error("the history still holds pages");
  });
});

/** The fingerprint unlocks the vault. */
const FINGERPRINT = {
  biometric: "fingerprint",
  biometricUnlock: true,
  deviceUnlock: true,
} as const;

async function openExport(user: ReturnType<typeof renderApp>["user"]) {
  await user.click(screen.getByTestId("codes-settings"));
  await user.click(await screen.findByTestId("settings-export"));
  expect(await screen.findByTestId("page-export")).toBeInTheDocument();
}

describe("exporting on the phone", () => {
  it("shows the migration codes behind the master password, and closes the session when done", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openExport(user);
    expect(
      within(screen.getByRole("radiogroup", { name: "导出到" })).getByRole("radio", {
        name: /Google 身份验证器/,
      }),
    ).toHaveAttribute("aria-checked", "true");
    const start = screen.getByTestId("export-start");
    expect(start).toBeDisabled();
    await user.type(screen.getByLabelText("主密码"), "wrong{Enter}");
    expect(await screen.findByText("密码错误")).toBeInTheDocument();
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    expect(await screen.findByTestId("page-export-view")).toBeInTheDocument();
    expect(await screen.findByRole("img", { name: "二维码" })).toBeInTheDocument();
    expect(screen.getByTestId("export-countdown")).toHaveTextContent("120");
    expect(
      within(screen.getByTestId("export-verify")).getAllByRole("listitem").length,
    ).toBeGreaterThan(0);
    const started = backend.calls.find((c) => c.command === "export_start");
    expect(started).toMatchObject({ command: "export_start", target: "google" });
    await user.click(screen.getByTestId("export-finish"));
    expect(await screen.findByTestId("page-export")).toBeInTheDocument();
    await waitFor(() => expect(backend.calls.some((c) => c.command === "export_close")).toBe(true));
  });

  it("closes the session when the codes are left with the back gesture", async () => {
    const { user, backend } = renderApp();
    await ready();
    await openExport(user);
    await user.click(
      within(screen.getByRole("radiogroup", { name: "导出到" })).getByRole("radio", {
        name: /Microsoft Authenticator/,
      }),
    );
    await user.type(screen.getByLabelText("主密码"), `${MOCK_PASSWORD}{Enter}`);
    await screen.findByTestId("page-export-view");
    act(() => history.back());
    expect(await screen.findByTestId("page-export")).toBeInTheDocument();
    await waitFor(() => expect(backend.calls.at(-1)).toMatchObject({ command: "export_close" }));
  });

  it("saves a plain list once its plain text is acknowledged", async () => {
    const { user } = renderApp();
    await ready();
    await openExport(user);
    await user.click(
      within(screen.getByRole("radiogroup", { name: "导出到" })).getByRole("radio", {
        name: /otpauth 列表文件/,
      }),
    );
    await user.type(screen.getByLabelText("主密码"), MOCK_PASSWORD);
    const save = screen.getByTestId("export-start");
    expect(save).toHaveTextContent("保存文件…");
    expect(save).toBeDisabled();
    await user.click(within(screen.getByTestId("export-plain-ok")).getByRole("switch"));
    await user.click(save);
    expect(await screen.findByRole("status")).toHaveTextContent("已保存：lockra-export.txt");
    expect(screen.getByTestId("page-export")).toBeInTheDocument();
  });

  it("takes the fingerprint for the password left empty", async () => {
    const { user, backend } = renderApp({ mock: FINGERPRINT });
    await ready();
    await openExport(user);
    // Nothing asked by itself: the page is a form.
    expect(backend.biometricReasons).toEqual([]);
    expect(screen.getByText("留空则用指纹验证。")).toBeInTheDocument();
    await user.click(screen.getByTestId("export-start"));
    expect(await screen.findByTestId("page-export-view")).toBeInTheDocument();
    expect(backend.biometricReasons).toEqual(["导出账号"]);
    expect(backend.calls.find((c) => c.command === "export_start")).not.toHaveProperty("password");
    await user.click(screen.getByTestId("export-finish"));
    expect(await screen.findByTestId("page-export")).toBeInTheDocument();
    // The plain list the same way, once acknowledged.
    await user.click(
      within(screen.getByRole("radiogroup", { name: "导出到" })).getByRole("radio", {
        name: /otpauth 列表文件/,
      }),
    );
    await user.click(within(screen.getByTestId("export-plain-ok")).getByRole("switch"));
    await user.click(screen.getByTestId("export-start"));
    expect(await screen.findByRole("status")).toHaveTextContent("已保存：lockra-export.txt");
    expect(backend.biometricReasons).toEqual(["导出账号", "导出账号"]);
  });

  it("lists the accounts a target cannot take, unticked, with the reason", async () => {
    const { user } = renderApp();
    await ready();
    await openExport(user);
    await user.click(
      within(screen.getByRole("radiogroup", { name: "导出到" })).getByRole("radio", {
        name: /Microsoft Authenticator/,
      }),
    );
    const boxes = within(screen.getByTestId("export-entries")).getAllByRole("checkbox");
    const blocked = boxes.filter((box) => box.hasAttribute("disabled"));
    expect(blocked.length).toBeGreaterThan(0);
    for (const box of blocked) expect(box).not.toBeChecked();
    expect(screen.getAllByText(/^不能导出：/).length).toBe(blocked.length);
    // Unticking one counts it out.
    const before = screen.getByTestId("export-count").textContent;
    const open = boxes.find((box) => !box.hasAttribute("disabled"));
    if (!open) throw new Error("no account can go to Microsoft Authenticator");
    await user.click(open);
    expect(screen.getByTestId("export-count").textContent).not.toBe(before);
  });
});
