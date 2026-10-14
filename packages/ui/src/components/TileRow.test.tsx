import { render, screen } from "@testing-library/react";
import { TileRow } from "./TileRow";

describe("TileRow", () => {
  it("shows the name, its status line, the actions and a note of its own", () => {
    render(
      <TileRow
        icon="browser"
        title="Chrome"
        subtitle="已连接"
        actions={<button type="button">写入</button>}
        note="尚未获得网站访问权限"
        data-testid="row"
      />,
    );
    const row = screen.getByTestId("row");
    expect(row).toHaveTextContent("Chrome已连接写入尚未获得网站访问权限");
    expect(screen.getByRole("button", { name: "写入" })).toBeInTheDocument();
  });

  it("is the name alone when nothing else is given", () => {
    render(<TileRow icon="puzzle" title="Edge" data-testid="row" />);
    expect(screen.getByTestId("row")).toHaveTextContent(/^Edge$/);
  });
});
