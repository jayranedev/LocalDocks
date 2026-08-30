/**
 * Captures the canonical LocalDocks screenshot set from the RUNNING PRODUCTION
 * BUILD over the Chrome DevTools Protocol.
 *
 * Every image comes from the real application rendering real data. Nothing is
 * mocked, composed or retouched. The set is reproducible: same script, same
 * demo environment, same viewport.
 *
 * Usage (PowerShell), from the repository root:
 *
 *   ./scripts/demo-environment.ps1                       # start the demo services
 *   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9448"
 *   Start-Process "$env:LOCALAPPDATA\\LocalDocks\\LocalDocks.exe"
 *   node ./scripts/capture-screenshots.mjs 9448
 *   ./scripts/demo-environment.ps1 -Stop
 *
 * The debugging port is a capture-time affordance only. Shipped builds set no
 * such variable, and LocalDocks itself never opens a debugging port.
 */
import { writeFileSync, mkdirSync } from 'node:fs';

const PORT = process.argv[2] ?? '9448';
const OUT = process.argv[3] ?? new URL('../docs/assets/screenshots/', import.meta.url).pathname.replace(/^\//, '');
mkdirSync(OUT, { recursive: true });

const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
const page = list.find(p => p.type === 'page') || list[0];
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const send = (method, params = {}) => new Promise(r => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = e => send('Runtime.evaluate', { expression: e, returnByValue: true, awaitPromise: true }).then(r => r?.result?.value);
ws.onmessage = m => { const d = JSON.parse(m.data); if (pending.has(d.id)) { pending.get(d.id)(d.result); pending.delete(d.id); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));

const nav = t => ev(`(()=>{const b=[...document.querySelectorAll('button,a')].find(e=>e.innerText.trim().startsWith(${JSON.stringify(t)})); if(b){b.click();return 'ok';} return 'not found: '+${JSON.stringify(t)};})()`);
const setMode = want => ev(`(()=>{const s=document.querySelector('[role="switch"]'); if(!s) return 'no switch';
  const isDev = s.getAttribute('aria-checked')==='true';
  if (isDev !== ${'${want}'}) s.click();
  return s.getAttribute('aria-checked');})()`.replace('${want}', want));
const setTheme = t => ev(`(()=>{document.documentElement.setAttribute('data-theme','${t}'); return '${t}';})()`);

async function shot(name, note, { top = true } = {}) {
  // Every frame starts at the top of its screen, so the set is consistent and
  // nothing is half-cut at the edge.
  if (top) { await ev('window.scrollTo(0,0)'); await sleep(250); }
  await sleep(1400);
  const r = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
  if (!r?.data) { console.log(`  FAIL  ${name}`); return; }
  writeFileSync(`${OUT}\\${name}.png`, Buffer.from(r.data, 'base64'));
  console.log(`  ok    ${name}.png   ${note}`);
}

ws.onopen = async () => {
  await sleep(2000);
  // A fixed viewport so every image in the set is the same size.
  await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 2, mobile: false });
  await sleep(1200);

  console.log('Capturing from the production build:');

  await setTheme('local-dark');
  await setMode(true);  await nav('Overview');
  await shot('01-overview-developer', 'Overview, Developer mode, Local Dark');

  await nav('Services');   await shot('02-services-developer', 'Services, Developer mode');
  await nav('Ports');      await shot('03-ports-developer', 'Ports, Developer mode');
  await nav('Processes');  await shot('04-processes-developer', 'Processes, Developer mode');

  await setMode(false);
  await nav('Overview');   await shot('05-overview-system', 'Overview, System mode');
  await nav('Services');   await shot('06-services-system', 'Services, System mode - classification chips');
  await nav('Processes');  await shot('07-processes-system', 'Processes, System mode - the full machine');
  await nav('Ports');
  // On a real machine the unfiltered socket table lists the host's LAN address
  // and its link-local IPv6 on the NetBIOS/SSDP rows. That is machine-specific
  // information that must not ship in public documentation, so this frame is
  // narrowed to loopback using the screen's own search box - a real control,
  // not a retouch - and the result is asserted below before it is kept.
  await sleep(1000);
  await ev(`(()=>{
    const i=[...document.querySelectorAll('input')].find(e=>/port, PID or process/i.test(e.placeholder||''));
    if(!i) return 'no input';
    const set=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;
    set.call(i,'127.0.0.1'); i.dispatchEvent(new Event('input',{bubbles:true}));
    return i.value;})()`);
  await sleep(1200);
  const routable = await ev(`(()=>{const t=document.body.innerText;
    const m=t.match(/\\b(?:10|127|192\\.168|172\\.(?:1[6-9]|2\\d|3[01]))\\.[0-9.]+|fe80::[0-9a-f:]+/gi)||[];
    return [...new Set(m)].filter(a=>!a.startsWith('127.0.0.1')).join(', ');})()`);
  if (routable !== '') { console.log(`  ABORT  08 still shows ${routable}`); process.exit(2); }
  await shot('08-ports-system', 'Ports, System mode, narrowed to loopback');
  await ev(`(()=>{const i=[...document.querySelectorAll('input')].find(e=>/port, PID or process/i.test(e.placeholder||''));
    const set=Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype,'value').set;
    set.call(i,''); i.dispatchEvent(new Event('input',{bubbles:true})); return 1;})()`);

  // Detail panel on a demo service.
  await setMode(true);
  await nav('Services'); await sleep(1200);
  const opened = await ev(`(()=>{const r=[...document.querySelectorAll('button')].find(b=>/node|mongod|python/i.test(b.innerText)&&b.innerText.includes('MB')); if(r){r.click();return 'ok';} return 'no row';})()`);
  console.log('  detail row:', opened);
  await shot('09-detail-panel', 'Detail panel with the classification reason');
  await ev(`(()=>{const b=[...document.querySelectorAll('aside button')].find(e=>(e.getAttribute('aria-label')||'')==='Close'); if(b)b.click(); return 1;})()`);

  // Telemetry, then themes.
  await nav('Overview'); await sleep(1000);
  // The one deliberate exception: this frame is the telemetry section itself.
  await ev(`(()=>{const s=document.querySelector('[aria-label="System telemetry"]'); if(s)s.scrollIntoView({block:'center'}); return 1;})()`);
  await shot('10-system-telemetry', 'All six telemetry cards', { top: false });
  await ev(`window.scrollTo(0,0)`);

  await setTheme('dark');       await shot('11-theme-dark', 'Overview, Dark');
  await setTheme('light');      await shot('12-theme-light', 'Overview, Light');
  await setTheme('local-dark');

  await nav('Settings');        await shot('13-settings', 'Settings');
  await nav('Overview');
  await send('Emulation.clearDeviceMetricsOverride');
  ws.close(); process.exit(0);
};
setTimeout(() => { console.log('TIMEOUT'); process.exit(1); }, 120000);
