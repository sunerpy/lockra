// @vitest-environment node
// The frame facts a request carries come from the browser only: the sender, the tab, and
// webNavigation's frame tree, checked against each other.
import type { FrameDetails, MessageSender } from "../browser/api";
import { chainOf, factsForFrame, factsFromSender, webUrl } from "./frames";

const FRAMES: FrameDetails[] = [
  { frameId: 0, parentFrameId: -1, url: "https://www.example.com/", documentId: "doc-0" },
  { frameId: 4, parentFrameId: 0, url: "https://www.example.com/frame", documentId: "doc-4" },
  { frameId: 7, parentFrameId: 4, url: "https://accounts.example.com/login", documentId: "doc-7" },
  { frameId: 9, parentFrameId: 0, url: "about:blank", documentId: "doc-9" },
  { frameId: 12, parentFrameId: 11, url: "https://lost.example/", documentId: "doc-12" },
];

const frames = async () => FRAMES;

function sender(change: Partial<MessageSender> = {}): MessageSender {
  return {
    tab: { id: 3, url: "https://www.example.com/" },
    frameId: 7,
    url: "https://accounts.example.com/login",
    origin: "https://accounts.example.com",
    documentId: "doc-7",
    ...change,
  };
}

describe("web addresses", () => {
  it("are http and https documents only", () => {
    expect(webUrl("https://example.com/a?b#c")).toBe("https://example.com/a?b#c");
    expect(webUrl("http://localhost:8080/")).toBe("http://localhost:8080/");
    const others = [
      "about:blank",
      "about:srcdoc",
      "data:text/html,x",
      "blob:https://example.com/1",
      "file:///etc/passwd",
      "chrome://settings",
      "javascript:alert(1)",
      "not a url",
      undefined,
      "",
    ];
    expect(others.map((url) => [url, webUrl(url)])).toEqual(others.map((url) => [url, null]));
  });
});

describe("the frame chain", () => {
  it("lists the ancestors, nearest first, without the page itself", () => {
    expect(chainOf(FRAMES, 7)).toEqual(["https://www.example.com/frame"]);
    expect(chainOf(FRAMES, 4)).toEqual([]);
    expect(chainOf(FRAMES, 0)).toEqual([]);
  });

  it("is refused when it breaks or loops", () => {
    expect(chainOf(FRAMES, 12)).toBeNull();
    expect(chainOf(FRAMES, 99)).toBeNull();
    const loop: FrameDetails[] = [
      { frameId: 1, parentFrameId: 2, url: "https://a.example/" },
      { frameId: 2, parentFrameId: 1, url: "https://b.example/" },
    ];
    expect(chainOf(loop, 1)).toBeNull();
  });
});

describe("a content script's request", () => {
  it("carries the sender's frame, the tab's page and the chain", async () => {
    expect(await factsFromSender(sender(), frames)).toEqual({
      tabId: 3,
      frameId: 7,
      documentId: "doc-7",
      facts: {
        frame_url: "https://accounts.example.com/login",
        top_url: "https://www.example.com/",
        frame_chain: ["https://www.example.com/frame"],
      },
    });
  });

  it("is refused for a frame that is not a web document", async () => {
    expect(
      await factsFromSender(
        sender({
          url: "about:blank",
          frameId: 9,
          documentId: "doc-9",
          origin: "https://www.example.com",
        }),
        frames,
      ),
    ).toEqual({ error: "not_web" });
    expect(await factsFromSender(sender({ url: "about:srcdoc" }), frames)).toEqual({
      error: "not_web",
    });
    expect(await factsFromSender(sender({ url: "data:text/html,<form>" }), frames)).toEqual({
      error: "not_web",
    });
    expect(
      await factsFromSender(sender({ url: "blob:https://accounts.example.com/x" }), frames),
    ).toEqual({ error: "not_web" });
  });

  it("is refused when the origin is not the URL's (a sandboxed or opaque frame)", async () => {
    expect(await factsFromSender(sender({ origin: "null" }), frames)).toEqual({ error: "origin" });
    expect(await factsFromSender(sender({ origin: "https://evil.example" }), frames)).toEqual({
      error: "origin",
    });
    // Firefox before 126 says no origin: the URL alone.
    expect(await factsFromSender(sender({ origin: undefined }), frames)).toMatchObject({
      frameId: 7,
    });
  });

  it("is refused when the frame navigated since (another document, or another URL without documentId)", async () => {
    expect(await factsFromSender(sender({ documentId: "doc-old" }), frames)).toEqual({
      error: "navigated",
    });
    expect(
      await factsFromSender(
        sender({
          documentId: undefined,
          url: "https://accounts.example.com/other",
          origin: "https://accounts.example.com",
        }),
        frames,
      ),
    ).toEqual({ error: "navigated" });
    expect(await factsFromSender(sender({ documentId: undefined }), frames)).toMatchObject({
      frameId: 7,
    });
  });

  it("is refused without a tab, a web page in it, the frame tree, or an unbroken chain", async () => {
    expect(await factsFromSender(sender({ tab: undefined }), frames)).toEqual({ error: "no_tab" });
    expect(
      await factsFromSender(sender({ tab: { id: 3, url: "chrome://newtab/" } }), frames),
    ).toEqual({ error: "not_web" });
    expect(await factsFromSender(sender({ tab: { id: 3 } }), frames)).toEqual({ error: "not_web" });
    expect(await factsFromSender(sender(), async () => null)).toEqual({ error: "frames" });
    expect(
      await factsFromSender(sender(), async () => {
        throw new Error("no tab");
      }),
    ).toEqual({ error: "frames" });
    expect(
      await factsFromSender(
        sender({
          frameId: 12,
          url: "https://lost.example/",
          origin: "https://lost.example",
          documentId: "doc-12",
        }),
        frames,
      ),
    ).toEqual({ error: "chain" });
    expect(await factsFromSender(sender({ frameId: undefined }), frames)).toEqual({
      error: "no_tab",
    });
  });

  it("leaves the chain out for a form in the page itself", async () => {
    const top = sender({
      frameId: 0,
      url: "https://www.example.com/",
      origin: "https://www.example.com",
      documentId: "doc-0",
    });
    expect(await factsFromSender(top, frames)).toEqual({
      tabId: 3,
      frameId: 0,
      documentId: "doc-0",
      facts: {
        frame_url: "https://www.example.com/",
        top_url: "https://www.example.com/",
        frame_chain: [],
      },
    });
  });
});

describe("a frame picked by the background (the shortcut, the popup, a menu)", () => {
  it("takes its URL from the frame tree and checks the document", async () => {
    expect(
      await factsForFrame(
        { tabId: 3, frameId: 7, documentId: "doc-7", tabUrl: "https://www.example.com/" },
        frames,
      ),
    ).toEqual({
      tabId: 3,
      frameId: 7,
      documentId: "doc-7",
      facts: {
        frame_url: "https://accounts.example.com/login",
        top_url: "https://www.example.com/",
        frame_chain: ["https://www.example.com/frame"],
      },
    });
    expect(
      await factsForFrame(
        { tabId: 3, frameId: 7, documentId: "doc-1", tabUrl: "https://www.example.com/" },
        frames,
      ),
    ).toEqual({ error: "navigated" });
    expect(
      await factsForFrame(
        {
          tabId: 3,
          frameId: 7,
          url: "https://accounts.example.com/elsewhere",
          tabUrl: "https://www.example.com/",
        },
        frames,
      ),
    ).toEqual({ error: "navigated" });
    expect(
      await factsForFrame({ tabId: 3, frameId: 9, tabUrl: "https://www.example.com/" }, frames),
    ).toEqual({ error: "not_web" });
    expect(
      await factsForFrame({ tabId: 3, frameId: 99, tabUrl: "https://www.example.com/" }, frames),
    ).toEqual({ error: "chain" });
    expect(await factsForFrame({ tabId: 3, frameId: 0, tabUrl: undefined }, frames)).toEqual({
      error: "not_web",
    });
  });
});
