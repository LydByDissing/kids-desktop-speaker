import type { Meta, StoryObj } from "@storybook/preact";
import { SCENARIOS, ScenarioArgs, renderScenario } from "../scenarios";

// The whole app against a mocked speaker, one story per scenario in
// src/scenarios.tsx, to validate the UI without a speaker (DEC-045).

const meta: Meta<ScenarioArgs> = {
  title: "Speaker/App",
  render: (args) => renderScenario({ args }),
  args: { setUp: true, admin: true },
  argTypes: {
    initialTab: { control: "select", options: ["player", "radio", "messages", "settings"] },
  },
};
export default meta;
type Story = StoryObj<ScenarioArgs>;

const story = (id: string): Story => {
  const s = SCENARIOS.find((x) => x.id === id)!;
  const n = SCENARIOS.indexOf(s) + 1;
  return s.client
    ? { name: `${n}. ${s.name}`, render: () => renderScenario(s) }
    : { name: `${n}. ${s.name}`, args: s.args };
};

export const NewSpeakerSetup = story("setup");
export const SignIn = story("signin");
export const NothingPlaying = story("idle");
export const RadioPlaying = story("playing");
export const AtTheLimit = story("limit");
export const RadioAndPodcasts = story("radio");
export const Messages = story("messages");
export const NoMessages = story("no-messages");
export const Settings = story("settings");
export const UpdateReady = story("update");
export const ChildDevice = story("child");
export const English = story("english");
export const SlowSpeaker = story("slow");
export const Offline = story("offline");
