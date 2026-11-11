// Vitest runs component tests in jsdom.
import { cleanup } from "@testing-library/preact";

// Unmount everything and let Preact's pending animation frames run before
// jsdom is torn down, or they fire into a window that no longer exists.
afterAll(async () => {
  cleanup();
  await new Promise((r) => setTimeout(r, 50));
});
