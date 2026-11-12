import { cleanup, render, screen } from "@testing-library/preact";
import { SCENARIOS, renderScenario } from "./scenarios";

// Storybook and the review page show these; each must get past loading.
afterEach(() => {
  cleanup();
  localStorage.clear();
});

const settled: Record<string, RegExp> = {
  setup: /Sæt højttaleren op/,
  signin: /Glemt adgang\?/,
  offline: /svarer ikke/,
  english: /Player/,
};

describe("review scenarios", () => {
  it("have unique ids", () => {
    expect(new Set(SCENARIOS.map((s) => s.id)).size).toBe(SCENARIOS.length);
  });

  for (const s of SCENARIOS.filter((x) => x.id !== "slow")) {
    it(`${s.id}: ${s.name}`, async () => {
      render(renderScenario(s));
      expect((await screen.findAllByText(settled[s.id] ?? /Afspiller/)).length).toBeGreaterThan(0);
    });
  }

  it("a child's device has two tabs", async () => {
    render(renderScenario(SCENARIOS.find((s) => s.id === "child")!));
    await screen.findAllByText("Afspiller");
    expect(screen.getAllByRole("button", { name: /Afspiller|Radio|Beskeder|Indstillinger/ }).filter((b) => b.classList.contains("tab"))).toHaveLength(2);
  });
});
