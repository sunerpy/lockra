import type { AccountColor } from "@lockra/shared";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { AccountAppearance } from "./AccountAppearance";

function Editor({ size }: { size?: "md" | "lg" }) {
  const [color, setColor] = useState<AccountColor>("auto");
  const [mark, setMark] = useState("");
  return (
    <AccountAppearance
      issuer="GitHub"
      account="octocat"
      color={color}
      mark={mark}
      onColor={setColor}
      onMark={setMark}
      size={size}
    />
  );
}

describe("AccountAppearance", () => {
  it("picks a colour from the swatches or the arrow keys, and keeps two characters of text", async () => {
    const user = userEvent.setup();
    render(<Editor />);
    const colours = screen.getByRole("radiogroup", { name: "颜色" });
    const auto = within(colours).getByRole("radio", { name: "自动" });
    expect(auto).toHaveAttribute("aria-checked", "true");
    // Before the first swatch comes the last one.
    await user.click(auto);
    await user.keyboard("{ArrowLeft}");
    const gray = within(colours).getByRole("radio", { name: "灰色" });
    expect(gray).toHaveAttribute("aria-checked", "true");
    expect(gray).toHaveFocus();
    expect(screen.getByTestId("entry-avatar")).toHaveAttribute("data-tag", "gray");
    await user.type(screen.getByLabelText("头像文字"), "👨‍💻🚀x");
    expect(screen.getByLabelText("头像文字")).toHaveValue("👨‍💻🚀");
  });

  it("comes in the phone's touch size", () => {
    const { unmount } = render(<Editor />);
    expect(screen.getByRole("radio", { name: "自动" })).toHaveClass("h-6", "w-6");
    unmount();
    render(<Editor size="lg" />);
    expect(screen.getByRole("radio", { name: "自动" })).toHaveClass("h-9", "w-9");
    expect(screen.getByLabelText("头像文字").closest("div")).toHaveClass("h-11");
  });
});
