// Folds the built review page (demo-dist/demo.html and its one script and
// stylesheet) into a single HTML file, so it can be opened or shared as one file.
import { readFileSync, writeFileSync } from "node:fs";

const dir = "demo-dist";
const html = readFileSync(`${dir}/demo.html`, "utf8");
const script = /<script[^>]*src="\.?\/?(assets\/[^"]+\.js)"[^>]*><\/script>/.exec(html);
const style = /<link[^>]*href="\.?\/?(assets\/[^"]+\.css)"[^>]*>/.exec(html);
if (!script || !style) throw new Error("demo-dist/demo.html has no script or stylesheet");
const js = readFileSync(`${dir}/${script[1]}`, "utf8").replace(/<\/script/gi, "<\\/script");
const css = readFileSync(`${dir}/${style[1]}`, "utf8");
const page = `<title>Kids Speaker Web UI</title>
<meta name="claude:viewport-fit-sides" content="cover">
<style>${css}</style>
<div id="demo"></div>
<script>document.body.classList.add("demo-body")</script>
<script type="module">${js}</script>
`;
writeFileSync(`${dir}/kids-speaker-ui.html`, page);
console.log(`${dir}/kids-speaker-ui.html: ${(page.length / 1024).toFixed(0)} KB`);
