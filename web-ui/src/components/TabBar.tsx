import { useApp } from "../context";

export type Tab = "player" | "radio" | "messages" | "settings";

const ICONS: Record<Tab, string> = { player: "▶", radio: "📻", messages: "🎙", settings: "⚙" };

export function TabBar({ tabs, current, onSelect }: { tabs: Tab[]; current: Tab; onSelect: (t: Tab) => void }) {
  const { t } = useApp();
  return (
    <nav class="tabbar" aria-label={t("tabs")}>
      {tabs.map((tab) => (
        <button
          key={tab}
          class={tab === current ? "tab active" : "tab"}
          aria-current={tab === current ? "page" : undefined}
          onClick={() => onSelect(tab)}
        >
          <span aria-hidden="true">{ICONS[tab]}</span>
          <span>{t(`tab.${tab}`)}</span>
        </button>
      ))}
    </nav>
  );
}
