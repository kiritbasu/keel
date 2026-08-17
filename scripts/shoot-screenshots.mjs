// Retake the README screenshots from the fixture corpus.
//
//   specline --home /tmp/shot/.specline migrate
//   specline --home /tmp/shot/.specline fixture
//   specline-daemon --home /tmp/shot/.specline --bind 127.0.0.1:7698 &
//   <chrome-headless-shell> --remote-debugging-port=9222 --user-data-dir=/tmp/shot/profile about:blank &
//   node scripts/shoot-screenshots.mjs
//
// Shot against Harbour, the fixture's invented billing product, rather than its
// own Specline project: that one is a snapshot of this project's early design
// and still argues for DuckDB and Lance, which would contradict
// docs/ARCHITECTURE.md on the next scroll.
//
// Why the DevTools Protocol rather than `chrome --screenshot`: the app holds an
// open event stream, so the page is never idle, `--virtual-time-budget` never
// fires and the screenshot never lands — it hangs with no error and no file.
// Waiting on a fixed timer is the point of this script, not an accident of it.
//
// The output is reproducible in layout but not byte for byte: the document
// page renders a relative timestamp ("revision 1, 43m ago"), so that one shot
// differs on every run while looking the same. Do not chase the diff.
//
// Needs no packages. Node 22 has a built-in WebSocket.
import { writeFileSync } from 'node:fs';

const PORT = process.env.CDP_PORT || 9222;
const BASE = process.env.APP || 'http://127.0.0.1:7698';
const OUT = process.env.OUT || 'docs/images';
const WIDTH = 1280;
const SCALE = 2;
const SETTLE_MS = 3500;

// Height is per-shot: the roadmap is short, and shooting it at 860 leaves the
// bottom half of the image empty.
const SHOTS = [
  ['overview', '#/projects/harbour', 860],
  ['board', '#/projects/harbour/board', 860],
  ['document', '#/projects/harbour/documents/spc_01M07P18VF911E6FSDWXZ1JVSY', 860],
  ['roadmap', '#/projects/harbour/roadmap', 540],
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function cdp(ws, id, method, params = {}, sessionId) {
  return new Promise((resolve, reject) => {
    const onMessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.id !== id) return;
      ws.removeEventListener('message', onMessage);
      message.error ? reject(new Error(`${method}: ${message.error.message}`)) : resolve(message.result);
    };
    ws.addEventListener('message', onMessage);
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    setTimeout(() => reject(new Error(`timed out calling ${method}`)), 20_000);
  });
}

const { webSocketDebuggerUrl } = await (await fetch(`http://127.0.0.1:${PORT}/json/version`)).json();
const ws = new WebSocket(webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});

let nextId = 1;
for (const [name, hash, height] of SHOTS) {
  const url = `${BASE}/${hash}`;
  const { targetId } = await cdp(ws, nextId++, 'Target.createTarget', { url });
  const { sessionId } = await cdp(ws, nextId++, 'Target.attachToTarget', { targetId, flatten: true });

  await cdp(ws, nextId++, 'Emulation.setDeviceMetricsOverride',
    { width: WIDTH, height, deviceScaleFactor: SCALE, mobile: false }, sessionId);
  // Hash routing means the first load can land on the default route, so the
  // route is set again here rather than trusted from createTarget.
  await cdp(ws, nextId++, 'Page.navigate', { url }, sessionId);
  await sleep(SETTLE_MS);

  const { data } = await cdp(ws, nextId++, 'Page.captureScreenshot',
    { format: 'png', captureBeyondViewport: false }, sessionId);
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(data, 'base64'));
  console.log(`wrote ${OUT}/${name}.png`);

  await cdp(ws, nextId++, 'Target.closeTarget', { targetId });
}
ws.close();
