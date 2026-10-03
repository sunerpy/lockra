import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
  // The appearance hook writes to <html>; each test starts without a theme.
  delete document.documentElement.dataset.theme;
});
