import assert from 'node:assert/strict';
import { writeFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const base = process.env.WEBDRIVER_URL || 'http://127.0.0.1:4444';
const application = resolve('src-tauri/target/debug/omnislice.exe');
const output = resolve('.workflow/native-evidence');
await mkdir(output, { recursive: true });
const evidence = { started: new Date().toISOString(), checks: [], limitations: ['原生鼠标、旧 DWM 缓存、实际音频输出、IME 与多显示器需独立验收'] };
const execute = promisify(execFile);
const probe = resolve('scripts/NativeProbe.cs').replaceAll("'", "''");
async function native(expression) {
  const { stdout } = await execute('powershell.exe', ['-NoProfile', '-Command', `Add-Type -Path '${probe}'; ${expression}`], { windowsHide: true, timeout: 15000 });
  return stdout.trim() ? JSON.parse(stdout) : undefined;
}
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let session;
async function request(method, path, body) {
  const response = await fetch(`${base}${path}`, { method, headers: { 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(45000) });
  const data = await response.json();
  if (!response.ok) throw new Error(JSON.stringify(data.value));
  return data.value;
}
async function command(method, path, body) { return request(method, `/session/${session}${path}`, body); }
async function evaluate(script, ...args) {
  return command('POST', '/execute/async', { script: `const done=arguments[arguments.length-1]; Promise.resolve().then(async()=>{${script}}).then(value=>done({ok:true,value}),error=>done({ok:false,error:String(error)}));`, args });
}
async function value(script, ...args) { const result = await evaluate(script, ...args); if (!result.ok) throw new Error(result.error); return result.value; }
async function invoke(name, args = {}) { return value('return await window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]);', name, args); }
async function find(label) {
  for (let attempt = 0; attempt < 30; attempt++) {
    for (const handle of await command('GET', '/window/handles')) {
      await command('POST', '/window', { handle });
      const actual = await value('return (window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label || "") + " " + location.href;');
      if (actual === label || actual.includes(label)) return handle;
    }
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  throw new Error(`WebView not found: ${label}`);
}
async function check(name, work, cleanup) {
  try { await work(); evidence.checks.push({ name, status: 'pass' }); console.log(`PASS ${name}`); }
  catch(error) { evidence.checks.push({ name, status: 'fail', error: String(error) }); process.exitCode=1; console.error(`FAIL ${name}: ${error}`); if(cleanup)await cleanup(); }
}

try {
  if (process.env.DEBUGGER_ADDRESS) {
    const endpoint = await fetch(`http://${process.env.DEBUGGER_ADDRESS}/json/version`, { signal: AbortSignal.timeout(3000) });
    if (!endpoint.ok) throw new Error('WebView2 debugging endpoint is unavailable; keep a Slice window open.');
  }
  const capabilities = process.env.DEBUGGER_ADDRESS
    ? { browserName: 'webview2', 'ms:edgeChromium': true, 'ms:edgeOptions': { debuggerAddress: process.env.DEBUGGER_ADDRESS } }
    : { 'tauri:options': { application } };
  const result = await request('POST', '/session', { capabilities: { alwaysMatch: capabilities } });
  session = result.sessionId;
  await command('POST', '/timeouts', { script: 10000 });
  const control = await find('control-window-002');
  let host;
  await check('两个原生窗口的设置隔离', async () => {
    const hosts = await invoke('list_windows');
    host = hosts.find(item => item.id === 'window-002');
    assert.ok(host);
    host = await invoke('update_window_options', { windowId: host.id, options: { ...host.options, privacyEnabled: false, manualHidden: false, manualPassthrough: false, topmost: false } });
    assert.equal(hosts.some(item => item.id === 'window-001'), false);
    const rejected = await evaluate('return await window.__TAURI_INTERNALS__.invoke("update_window_options", {windowId:"window-001",options:arguments[0]});', host.options);
    assert.equal(rejected.ok, false);
  });
  await check('真实网页子 WebView 打开', async () => {
    await invoke('open_slice', { windowId: host.id, url: 'http://127.0.0.1:4174/' });
    await find('content-window-002');
    assert.equal(await value('return document.querySelector("h1")?.textContent;'), '实时数据切片');
    assert.equal(await value('return innerWidth;'), 1280);
    assert.equal(await value('return innerHeight;'), 900);
  });
  await check('远程页面拒绝特权命令', async () => {
    const result = await evaluate('if(!window.__TAURI_INTERNALS__)throw new Error("IPC unavailable"); return await window.__TAURI_INTERNALS__.invoke("list_windows");');
    assert.equal(result.ok, false);
  });
  await command('POST', '/window', { handle: control });
  await check('文字透明度不累乘且动态继承正确', async () => {
    await invoke('update_window_options', { windowId: host.id, options: { ...host.options, textAlpha: 0.5, backgroundAlpha: 0.5 } });
    await find('content-window-002');
    await new Promise(resolve => setTimeout(resolve, 150));
    const first = await value('return getComputedStyle(document.querySelector("#nested")).color;');
    assert.match(first, /0\.3\)/);
    assert.match(await value('return getComputedStyle(document.body).backgroundColor;'), /0\.4\)/);
    await value('window.__omni.alpha(0.5,0.5); return true;');
    await new Promise(resolve => setTimeout(resolve, 150));
    assert.equal(await value('return getComputedStyle(document.querySelector("#nested")).color;'), first);
    await value('document.querySelector("#inherit").append(Object.assign(document.createElement("span"),{id:"new-child",textContent:"新增"})); return true;');
    await new Promise(resolve => setTimeout(resolve, 150));
    assert.equal(await value('return getComputedStyle(document.querySelector("#new-child")).color;'), first);
    await value('window.__omni.alpha(1,1); return true;');
    await delay(150);
    assert.match(await value('return getComputedStyle(document.querySelector("#nested")).color;'), /0\.6\)/);
    assert.match(await value('return getComputedStyle(document.body).backgroundColor;'), /0\.8\)/);
  });
  await command('POST', '/window', { handle: control });
  await check('原生矩形框选及固定布局视口', async () => {
    await native('[OmniSliceProbe]::Place("window-002")');
    host = await invoke('update_window_options', { windowId: host.id, options: { ...host.options, topmost: true } });
    await invoke('begin_crop', { windowId: host.id });
    await delay(300);
    await find('content-window-002');
    evidence.selectorReady = await value('window.__omniTest=[];window.addEventListener("pointerdown",e=>window.__omniTest.push({type:e.type,x:e.clientX,y:e.clientY,trusted:e.isTrusted,target:e.target.id}),true);window.addEventListener("pointerup",e=>window.__omniTest.push({type:e.type,x:e.clientX,y:e.clientY,trusted:e.isTrusted,target:e.target.id}),true);return {exists:!!document.getElementById("omnislice-selector"),target:document.elementFromPoint(160,150)?.id};');
    await command('POST', '/window', { handle: control });
    const slot = await value('const r=document.querySelector(".viewport").getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height,dpi:devicePixelRatio};');
    const state = await native('[OmniSliceProbe]::State("window-002")');
    const scale = (await invoke('window_diagnostics', { windowId: host.id })).contentScale;
    const left = state.client[0] + slot.x * slot.dpi + (slot.width * slot.dpi - 1280 * scale) / 2;
    const top = state.client[1] + slot.y * slot.dpi + (slot.height * slot.dpi - 900 * scale) / 2;
    evidence.cropInput = { slot, state, scale, left, top };
    evidence.nativeDrag = await native(`[OmniSliceProbe]::Drag("window-002",${Math.round(left + 160 * scale)},${Math.round(top + 150 * scale)},${Math.round(left + 880 * scale)},${Math.round(top + 630 * scale)})`);
    await delay(500);
    await find('content-window-002');
    evidence.pagePointerEvents = await value('return window.__omniTest;');
    await command('POST', '/window', { handle: control });
    evidence.cropAfterInput = await native('[OmniSliceProbe]::State("window-002")');
    host = (await invoke('list_windows')).find(item => item.id === host.id);
    assert.ok(Math.abs(host.slices[0].crop.x - 160) < 4, JSON.stringify(host.slices[0].crop));
    evidence.cropResult = host.slices[0].crop;
    assert.ok(Math.abs(host.slices[0].crop.width - 720) < 6, JSON.stringify(evidence.cropInput));
    await find('content-window-002');
    assert.equal(await value('return innerWidth;'), 1280);
  }, async () => {
    await command('POST', '/window', { handle: control });
    await invoke('open_slice', { windowId: host.id, url: 'http://127.0.0.1:4174/' });
  });
  await command('POST', '/window', { handle: control });
  let other;
  await check('新增常显窗口', async () => {
    other = await invoke('create_window', { mode: 'always-visible' });
    assert.equal(other.options.privacyEnabled, false);
  });
  await check('移出隐藏、原生穿透及静音恢复', async () => {
    const before = await invoke('window_diagnostics', { windowId: host.id });
    host = await invoke('update_window_options', { windowId: host.id, options: { ...host.options, privacyEnabled: true } });
    const roundTrip = await native('[OmniSliceProbe]::HoverRoundTrip("window-002")');
    evidence.hoverRoundTrip = roundTrip;
    assert.equal(roundTrip.outMoved, true);
    assert.equal(roundTrip.inMoved, true);
    const hidden = roundTrip.hidden;
    evidence.hiddenState = hidden;
    assert.equal(hidden.region, 1);
    assert.equal(hidden.styles & 32, 32);
    evidence.checks.push({ name: 'DWM 当前及缓存预览遮蔽', status: 'not_run', reason: '属性仅支持 setter；需实际任务栏、Alt+Tab 和 Peek 图像验收，不能用失败的 getter 零值判断', getterResults: [hidden.forceResult, hidden.bitmapResult] });
    const focused = hidden.foreground;
    const visible = roundTrip.visible;
    evidence.visibleState = visible;
    assert.notEqual(visible.region, 1);
    assert.equal(visible.styles & 32, 0);
    assert.equal(visible.foreground, focused);
    assert.equal((await invoke('window_diagnostics', { windowId: host.id })).muted, before.muted);
    const muted = await native('[OmniSliceProbe]::MoveAndState("window-002",10,10)');
    assert.equal(muted.state.region, 1);
    assert.equal((await invoke('window_diagnostics', { windowId: host.id })).muted, true);
    await native('[OmniSliceProbe]::Recover()');
    await delay(400);
    assert.notEqual((await native('[OmniSliceProbe]::State("window-002")')).region, 1);
  });
  await check('重复置顶与独立取消入口', async () => {
    host = (await invoke('list_windows')).find(item => item.id === host.id);
    await invoke('update_window_options', { windowId: host.id, options: { ...host.options, topmost: true } });
    await invoke('raise_window', { windowId: host.id });
    assert.equal((await native('[OmniSliceProbe]::State("window-002")')).styles & 8, 8);
    await invoke('update_window_options', { windowId: host.id, options: { ...host.options, topmost: false } });
    assert.equal((await native('[OmniSliceProbe]::State("window-002")')).styles & 8, 0);
  });
  await check('不同窗口比例下的原生点击映射', async () => {
    host = (await invoke('list_windows')).find(item => item.id === host.id);
    host = await invoke('update_window_options', { windowId: host.id, options: { ...host.options, privacyEnabled: false, topmost: true } });
    const content = await find('content-window-002');
    await value('Object.assign(document.querySelector("#count").style,{position:"fixed",left:"300px",top:"300px"});return true;');
    evidence.clickMappings = [];
    for (const [width,height] of [[420,650],[800,560],[556,789]]) {
      await native(`[OmniSliceProbe]::Resize("window-002",${width},${height})`);
      await delay(400);
      await command('POST', '/window', { handle: content });
      const point = await value('const r=document.querySelector("#count").getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,before:document.querySelector("#count").textContent,viewport:[innerWidth,innerHeight]};');
      assert.deepEqual(point.viewport,[1280,900]);
      await command('POST', '/window', { handle: control });
      const slot = await value('const r=document.querySelector(".viewport").getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,dpi:devicePixelRatio};');
      const state = await native('[OmniSliceProbe]::State("window-002")');
      const scale = (await invoke('window_diagnostics',{windowId:host.id})).contentScale;
      const crop = host.slices[0].crop;
      const x = Math.round(state.client[0]+slot.x*slot.dpi+(slot.width*slot.dpi-crop.width*scale)/2+(point.x-crop.x)*scale);
      const y = Math.round(state.client[1]+slot.y*slot.dpi+(slot.height*slot.dpi-crop.height*scale)/2+(point.y-crop.y)*scale);
      await native(`[OmniSliceProbe]::Drag("window-002",${x},${y},${x},${y})`);
      await command('POST', '/window', { handle: content });
      const after = await value('return document.querySelector("#count").textContent;');
      assert.notEqual(after,point.before);
      evidence.clickMappings.push({width,height,scale,x,y,before:point.before,after});
    }
    await command('POST', '/window', { handle: control });
  });
  await check('网页拖出窗口松手及其他窗口按下隔离', async () => {
    host = await invoke('update_window_options',{windowId:host.id,options:{...host.options,privacyEnabled:true,topmost:true}});
    const drag = await native('[OmniSliceProbe]::HoldOutside("window-002",350,450)');
    evidence.pointerCapture = drag;
    assert.equal(drag.held.region,0);
    assert.equal(drag.gui.captureRoot,drag.held.hwnd);
    assert.equal(drag.released.region,1);
    other = await invoke('update_window_options',{windowId:other.id,options:{...other.options,privacyEnabled:false,topmost:true}});
    await native(`[OmniSliceProbe]::PlaceAt("${other.id}",800,80)`);
    const isolated = await native(`[OmniSliceProbe]::OtherPress("window-002","${other.id}",1100,110)`);
    evidence.otherWindowPress = isolated;
    assert.equal(isolated.region,1);
  });
  await check('多个置顶窗口的实际层序', async () => {
    host = await invoke('update_window_options',{windowId:host.id,options:{...host.options,privacyEnabled:false,topmost:true}});
    await invoke('raise_window',{windowId:host.id});
    other = await invoke('update_window_options',{windowId:other.id,options:{...other.options,topmost:false}});
    other = await invoke('update_window_options',{windowId:other.id,options:{...other.options,topmost:true}});
    assert.deepEqual(await native(`[OmniSliceProbe]::Order("${host.id}","${other.id}")`),[other.id,host.id]);
    await invoke('raise_window',{windowId:host.id});
    assert.deepEqual(await native(`[OmniSliceProbe]::Order("${host.id}","${other.id}")`),[host.id,other.id]);
    await invoke('raise_window',{windowId:other.id});
    await invoke('update_window_options',{windowId:host.id,options:{...host.options,textAlpha:0.7}});
    assert.deepEqual(await native(`[OmniSliceProbe]::Order("${host.id}","${other.id}")`),[other.id,host.id]);
  });
  await writeFile(resolve(output, 'webview.png'), Buffer.from(await command('GET', '/screenshot'), 'base64'));
} catch (error) {
  evidence.checks.push({ name: 'suite', status: 'fail', error: String(error) });
  process.exitCode = 1;
  console.error(error);
} finally {
  evidence.finished = new Date().toISOString();
  await writeFile(resolve(output, `smoke-${evidence.started.replaceAll(/[:.]/g, '-')}.json`), JSON.stringify(evidence, null, 2));
  await writeFile(resolve(output, 'smoke.json'), JSON.stringify(evidence, null, 2));
  if (session) await request('DELETE', `/session/${session}`).catch(error => console.error(`Session cleanup: ${error}`));
}
