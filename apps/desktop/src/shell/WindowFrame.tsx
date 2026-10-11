import { type Platform } from "@lockra/shared";
import { TitleBar, cx } from "@lockra/ui";
import type { ReactNode } from "react";
import { useWindowChrome } from "../app/window";

export interface WindowFrameProps {
  /** The window's name in the title bar; the unlocked shell leaves it to the page's header. */
  title?: ReactNode;
  platform: Platform | undefined;
  right?: ReactNode;
  /** The title bar starts at the window's edge (no sidebar beside it). */
  bare?: boolean;
  children: ReactNode;
  className?: string;
}

/** The frameless window's chrome: the 40 px title bar (drag strip and window buttons) over the
 *  body, which alone scrolls (`min-h-0`). */
export function WindowFrame({
  title,
  platform,
  right,
  bare = false,
  children,
  className,
}: WindowFrameProps) {
  const chrome = useWindowChrome(platform);
  return (
    <div className={cx("grid h-full min-w-0 grid-rows-[auto_minmax(0,1fr)]", className)}>
      <TitleBar
        title={title}
        right={right}
        platform={chrome.platform}
        controls={chrome.controls}
        maximized={chrome.maximized}
        trafficLights={bare && chrome.platform === "macos"}
      />
      {children}
    </div>
  );
}
