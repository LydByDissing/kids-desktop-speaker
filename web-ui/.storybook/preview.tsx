import type { Preview } from "@storybook/preact";
import "../src/styles.css";

const preview: Preview = {
  parameters: {
    layout: "fullscreen",
    viewport: { defaultViewport: "mobile2" },
  },
};
export default preview;
