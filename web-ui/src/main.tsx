import { render } from "preact";
import { App } from "./App";
import { HttpClient } from "./api/client";
import "./styles.css";

render(<App client={new HttpClient()} />, document.getElementById("app")!);
