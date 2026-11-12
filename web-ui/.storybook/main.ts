import type { StorybookConfig } from "@storybook/preact-vite";

// Every screen and state against a mocked API, to validate the UI without a
// speaker (DEC-045).
const config: StorybookConfig = {
  stories: ["../src/**/*.stories.tsx"],
  addons: ["@storybook/addon-essentials"],
  framework: { name: "@storybook/preact-vite", options: {} },
  core: { disableTelemetry: true },
};
export default config;
