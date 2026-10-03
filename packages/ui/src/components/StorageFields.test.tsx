import { type StorageForm, emptyStorageForm } from "@lockra/shared";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { StorageFields } from "./StorageFields";

function Form({ size, onForm }: { size?: "md" | "lg"; onForm: (form: StorageForm) => void }) {
  const [form, setForm] = useState(emptyStorageForm);
  return (
    <StorageFields
      form={form}
      size={size}
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
});
