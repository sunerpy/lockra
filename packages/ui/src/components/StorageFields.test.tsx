import { type ErrorCode, LockraError, type StorageForm, emptyStorageForm } from "@lockra/shared";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { StorageFields } from "./StorageFields";

function Form({
  size,
  onForm,
  failure,
  pickFolder,
}: {
  size?: "md" | "lg";
  onForm: (form: StorageForm) => void;
  failure?: ErrorCode;
  pickFolder?: () => Promise<string | null>;
}) {
  const [form, setForm] = useState(emptyStorageForm);
  return (
    <StorageFields
      form={form}
      size={size}
      failure={failure}
      pickFolder={pickFolder}
      onChange={(patch) =>
        setForm((current) => {
          const next = { ...current, ...patch };
          onForm(next);
          return next;
        })
      }
    />
  );
}

describe("StorageFields", () => {
  it("offers a cloud drive's folder where the folder dialog is, and shows the one it chose", async () => {
    const user = userEvent.setup();
    let form = emptyStorageForm();
    const answers: (string | null | LockraError)[] = [
      null,
      new LockraError("sync_folder_missing"),
      "C:\\Users\\me\\OneDrive\\Lockra",
    ];
    const pickFolder = async () => {
      const answer = answers.shift() ?? null;
      if (answer instanceof LockraError) throw answer;
      return answer;
    };
    render(<Form onForm={(next) => (form = next)} pickFolder={pickFolder} />);
    await user.click(screen.getByRole("radio", { name: "网盘文件夹" }));
    expect(form).toMatchObject({ kind: "folder", preset: "folder", folder: "" });
    expect(screen.getByText("还没有选择文件夹")).toBeInTheDocument();
    // Nothing typed: no address, no credentials, no folder inside.
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("文件夹（可选）")).not.toBeInTheDocument();
    // Cancelled: nothing chosen. Then a folder that is no longer there. Then one.
    await user.click(screen.getByRole("button", { name: "选择文件夹…" }));
    expect(form.folder).toBe("");
    await user.click(screen.getByRole("button", { name: "选择文件夹…" }));
    expect(screen.getByRole("alert")).toHaveTextContent("找不到同步文件夹");
    await user.click(screen.getByRole("button", { name: "选择文件夹…" }));
    expect(form.folder).toBe("C:\\Users\\me\\OneDrive\\Lockra");
    expect(screen.getByTestId("storage-folder-path")).toHaveTextContent("OneDrive");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "更换文件夹…" })).toBeInTheDocument();
  });

  it("offers no folder without the folder dialog (the phone)", () => {
    render(<Form onForm={() => undefined} />);
    expect(screen.queryByRole("radio", { name: "网盘文件夹" })).not.toBeInTheDocument();
  });

  it("asks for an S3 bucket's settings, or a WebDAV folder's", async () => {
    const user = userEvent.setup();
    let form = emptyStorageForm();
    render(<Form size="lg" onForm={(next) => (form = next)} />);
    await user.type(screen.getByLabelText("服务地址"), "https://s3.example.com");
    await user.type(screen.getByLabelText("访问密钥"), "secret");
    await user.click(screen.getByRole("switch", { name: "路径式访问" }));
    expect(form).toMatchObject({
      kind: "s3",
      endpoint: "https://s3.example.com",
      secretAccessKey: "secret",
      pathStyle: true,
    });
    await user.click(screen.getByRole("radio", { name: "WebDAV" }));
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "路径式访问" })).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("WebDAV 地址"), "https://dav.example.com/");
    await user.type(screen.getByLabelText("用户名"), "me");
    await user.type(screen.getByLabelText("密码"), "pw");
    await user.clear(screen.getByLabelText("文件夹（可选）"));
    expect(form).toMatchObject({
      kind: "webdav",
      url: "https://dav.example.com/",
      username: "me",
      password: "pw",
      prefix: "",
    });
  });

  it("keeps the China regions of AWS to their own provider and shows the address it makes", async () => {
    const user = userEvent.setup();
    let form = emptyStorageForm();
    render(<Form onForm={(next) => (form = next)} />);
    await user.selectOptions(screen.getByLabelText("服务商"), "aws-cn");
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "路径式访问" })).not.toBeInTheDocument();
    const region = screen.getByLabelText("区域");
    expect([...region.querySelectorAll("option")].map((o) => o.value)).toEqual([
      "cn-north-1",
      "cn-northwest-1",
    ]);
    expect(screen.getByTestId("storage-address")).toHaveTextContent(
      "https://s3.cn-north-1.amazonaws.com.cn",
    );
    await user.selectOptions(region, "cn-northwest-1");
    expect(form).toMatchObject({
      preset: "aws-cn",
      region: "cn-northwest-1",
      endpoint: "https://s3.cn-northwest-1.amazonaws.com.cn",
      pathStyle: false,
    });
    expect(screen.getByTestId("storage-preset-hint")).toHaveTextContent("北京（cn-north-1）");
    // A China region typed under the global provider is refused, with the way out.
    await user.selectOptions(screen.getByLabelText("服务商"), "aws");
    await user.clear(screen.getByLabelText("区域"));
    await user.type(screen.getByLabelText("区域"), "cn-north-1");
    expect(screen.getByText("这是中国区域，请改选「AWS S3（中国区域）」。")).toBeInTheDocument();
  });

  it("suggests a service's regions and asks R2 for its account only", async () => {
    const user = userEvent.setup();
    let form = emptyStorageForm();
    render(<Form onForm={(next) => (form = next)} />);
    await user.selectOptions(screen.getByLabelText("服务商"), "oss");
    const region = screen.getByLabelText("区域");
    const list = document.getElementById(region.getAttribute("list") ?? "");
    expect(list?.querySelectorAll("option").length).toBeGreaterThan(5);
    await user.type(region, "cn-hongkong");
    expect(form.endpoint).toBe("https://s3.oss-cn-hongkong.aliyuncs.com");
    await user.selectOptions(screen.getByLabelText("服务商"), "r2");
    expect(screen.queryByLabelText("区域")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("账户 ID"), "not an id");
    expect(screen.getByText("账户 ID 是 32 个十六进制字符。")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("账户 ID"));
    await user.type(screen.getByLabelText("账户 ID"), "0123456789abcdef0123456789abcdef");
    expect(form).toMatchObject({
      region: "auto",
      endpoint: "https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com",
    });
  });

  it("fills in a WebDAV service's address from its server and the user name", async () => {
    const user = userEvent.setup();
    let form = emptyStorageForm();
    render(<Form size="lg" onForm={(next) => (form = next)} />);
    await user.click(screen.getByRole("radio", { name: "WebDAV" }));
    await user.selectOptions(screen.getByLabelText("服务商"), "jianguoyun");
    expect(screen.queryByLabelText("WebDAV 地址")).not.toBeInTheDocument();
    expect(screen.getByTestId("storage-address")).toHaveTextContent(
      "https://dav.jianguoyun.com/dav/",
    );
    expect(screen.getByTestId("storage-preset-hint")).toHaveTextContent("应用密码");
    await user.selectOptions(screen.getByLabelText("服务商"), "nextcloud");
    await user.type(screen.getByLabelText("服务器地址"), "cloud.example.com");
    await user.type(screen.getByLabelText("用户名"), "me");
    expect(form.url).toBe("https://cloud.example.com/remote.php/dav/files/me/");
    await user.selectOptions(screen.getByLabelText("服务商"), "webdav-custom");
    expect(screen.getByLabelText("WebDAV 地址")).toHaveValue(
      "https://cloud.example.com/remote.php/dav/files/me/",
    );
  });

  it("says what a refusal most likely means for the chosen service", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<Form onForm={() => {}} failure="sync_denied" />);
    expect(screen.queryByTestId("storage-failure-hint")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("服务商"), "aws");
    expect(screen.getByTestId("storage-failure-hint")).toHaveTextContent("中国区域与全球区域");
    rerender(<Form onForm={() => {}} failure="sync_storage_failed" />);
    expect(screen.queryByTestId("storage-failure-hint")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("服务商"), "cos");
    expect(screen.getByTestId("storage-failure-hint")).toHaveTextContent("APPID");
    await user.selectOptions(screen.getByLabelText("服务商"), "oss");
    expect(screen.getByTestId("storage-failure-hint")).toHaveTextContent("2025 年 3 月 20 日");
    rerender(<Form onForm={() => {}} failure="sync_denied" />);
    await user.click(screen.getByRole("radio", { name: "WebDAV" }));
    await user.selectOptions(screen.getByLabelText("服务商"), "nextcloud");
    expect(screen.getByTestId("storage-failure-hint")).toHaveTextContent("应用密码");
  });
});
