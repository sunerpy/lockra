import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  // The channel's tests run on Node, without a document.
  if (typeof document === "undefined") return;
  cleanup();
  delete document.documentElement.dataset.theme;
});
