import { render } from "preact";
import { useState } from "preact/hooks";
import { SCENARIOS, renderScenario } from "../scenarios";
import "../styles.css";
import "./demo.css";

// A review page: the real app in a phone-sized frame against the mocked
// speaker, with every scenario one click away (DEC-045). Built by
// `npm run build-demo`; it never talks to a real speaker.

const KEY = "kds-demo-scenario";
const remembered = () => {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
};

type Theme = "system" | "light" | "dark";

function Demo() {
  const [id, setId] = useState(() => (SCENARIOS.some((s) => s.id === remembered()) ? remembered()! : "playing"));
  const [run, setRun] = useState(0);
  const [theme, setTheme] = useState<Theme>("system");
  const scenario = SCENARIOS.find((s) => s.id === id)!;

  const choose = (next: string) => {
    setId(next);
    setRun((r) => r + 1);
    try {
      localStorage.setItem(KEY, next);
    } catch {
      /* storage unavailable */
    }
  };
  const pickTheme = (next: Theme) => {
    setTheme(next);
    if (next === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", next);
  };

  return (
    <div class="demo">
      <header class="demo-head">
        <h1>Kids Speaker Web UI</h1>
        <p>
          The real app against a pretend speaker. Pick a scenario and click around: it remembers what you do until you
          start the scenario again. The setup code is <b>471293</b>. Recording uses a fake microphone.
        </p>
      </header>
      <nav class="demo-list" aria-label="Scenarios">
        <ol>
          {SCENARIOS.map((s) => (
            <li key={s.id}>
              <button class={s.id === id ? "on" : ""} aria-current={s.id === id ? "true" : undefined} onClick={() => choose(s.id)}>
                {s.name}
              </button>
            </li>
          ))}
        </ol>
        <div class="demo-tools">
          <button id="demo-restart" onClick={() => setRun((r) => r + 1)}>Start again</button>
          <label for="demo-theme">Theme</label>
          <select id="demo-theme" value={theme} onChange={(e) => pickTheme((e.target as HTMLSelectElement).value as Theme)}>
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </div>
      </nav>
      <section class="demo-stage" aria-label={scenario.name}>
        <div class="phone">
          <div class="phone-scroll">{renderScenario(scenario, `${id}-${run}`)}</div>
        </div>
        <p class="demo-caption">{scenario.name}</p>
      </section>
    </div>
  );
}

render(<Demo />, document.getElementById("demo")!);
