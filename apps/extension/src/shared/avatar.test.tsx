// The extension's avatar colours and letters are the app's (EntryAvatar), and its plain-DOM icons
// draw the app's paths.
import { Icon, autoColor as appColor, initial as appInitial } from "@lockra/ui";
import { render } from "@testing-library/react";
import { AUTO_COLORS, autoColor, initial } from "./avatar";
import { ICONS, icon } from "./icons";

describe("avatars", () => {
  it("take the app's colour and letter for a name", () => {
    const names = [
      "GitHub",
      "github",
      "招商银行",
      "Bank of Example",
      "",
      "  spaced  ",
      "😀 emoji",
      "Ünïcode",
      "a",
      "zz-top",
    ];
    expect(names.map((n) => autoColor(n, "alice"))).toEqual(names.map((n) => appColor(n, "alice")));
    expect(names.map((n) => initial(n, "alice"))).toEqual(names.map((n) => appInitial(n, "alice")));
    expect(AUTO_COLORS).not.toContain("gray");
  });
});

describe("icons", () => {
  it("draw the paths of the app's Icon", () => {
    for (const name of Object.keys(ICONS) as (keyof typeof ICONS)[]) {
      const { container, unmount } = render(<Icon name={name} />);
      expect({
        name,
        d: icon(document, name, 16).querySelector("path")?.getAttribute("d"),
      }).toEqual({
        name,
        d: container.querySelector("path")?.getAttribute("d"),
      });
      unmount();
    }
  });
});
