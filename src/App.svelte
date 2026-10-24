<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { HostWindow, WindowMode, WindowOptions } from './lib/types';
  import { validateWindowOptions } from './lib/window-options';

  const windowId = new URLSearchParams(location.search).get('windowId');
  let host = $state<HostWindow>();
  let error = $state('');
  let busy = $state(false);
  let url = $state('http://127.0.0.1:4174/');
  let message = $state('输入网址，打开原始页面后框选需要保留的区域。');
  let viewport: HTMLElement;

  async function action(work: () => Promise<void>) {
    busy = true;
    error = '';
    try { await work(); } catch (cause) { error = String(cause); }
    finally { busy = false; }
  }
  async function refresh() {
    const windows = await invoke<HostWindow[]>('list_windows');
    host = windows.find(item => item.id === windowId);
    if (!host) throw new Error('当前窗口未注册');
  }
  async function update(patch: Partial<WindowOptions>) {
    if (!host) return;
    const options = { ...host.options, ...patch };
    await action(async () => {
      const request = validateWindowOptions({ windowId, options }, host ? [host.id] : []);
      host = await invoke('update_window_options', request);
    });
  }
  async function create(mode: WindowMode) {
    await action(async () => { await invoke('create_window', { mode }); });
  }
  async function open() {
    await action(async () => {
      await invoke('open_slice', { windowId, url });
      message = '页面已打开。点击重新裁剪，在页面内拖出矩形。';
    });
  }
  onMount(() => {
    void action(refresh);
    const poll = setInterval(() => { if (!busy) void refresh().catch(cause => { error = String(cause); }); }, 1000);
    let timer: ReturnType<typeof setTimeout>;
    const observer = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        const { x, y, width, height } = viewport.getBoundingClientRect();
        if (width > 0 && height > 0) void invoke('set_content_bounds', { windowId, slot: { x, y, width, height } }).catch(cause => { error = String(cause); });
      }, 80);
    });
    observer.observe(viewport);
    return () => { observer.disconnect(); clearTimeout(timer); clearInterval(poll); };
  });
</script>

<svelte:head><title>OmniSlice · 幽灵窗</title></svelte:head>

<main>
  <header>
    <button class="brand" aria-label="拖动窗口" onpointerdown={(event) => {
      if (event.button === 0) void action(() => getCurrentWindow().startDragging());
    }}><span class="mark">O</span><span>OmniSlice<small>幽灵窗 · {windowId}</small></span></button>
    <button class="quiet" aria-label="关闭窗口" onclick={() => void action(() => getCurrentWindow().close())}>关闭</button>
  </header>
  <section class="controls" aria-label="切片控制">
    <form onsubmit={(event) => { event.preventDefault(); void open(); }}>
      <label for="url">网页地址</label>
      <div class="row"><input id="url" type="url" bind:value={url} required /><button disabled={busy}>打开</button></div>
    </form>
    <div class="row actions">
      <button class="quiet" disabled={busy} onclick={() => void action(async () => { await invoke('begin_crop', { windowId }); message = '拖动框选，按 Esc 取消；再次点击裁剪按钮也可退出。'; })}>裁剪 / 退出</button>
      <button class="quiet" disabled={busy} onclick={() => host?.options.topmost ? void action(async () => { await invoke('raise_window', { windowId }); }) : void update({ topmost: true })}>{host?.options.topmost ? '提到最前' : '置顶'}</button>
      {#if host?.options.topmost}<button class="quiet" disabled={busy} onclick={() => void update({ topmost: false })}>取消置顶</button>{/if}
      <button class="quiet" disabled={busy} onclick={() => void create('always-visible')}>新窗口</button>
    </div>
    {#if host}
      <details>
        <summary>窗口设置 · {host.options.privacyEnabled ? '移出隐身' : '持续显示'}</summary>
        <div class="settings">
          <label><input type="checkbox" checked={host.options.privacyEnabled} disabled={busy} onchange={(event) => void update({ privacyEnabled: event.currentTarget.checked })} />鼠标移开隐身</label>
          <label><input type="checkbox" checked={host.options.manualPassthrough} disabled={busy} onchange={(event) => void update({ manualPassthrough: event.currentTarget.checked })} />鼠标穿透（热键恢复）</label>
          <label>文字透明度 <output>{Math.round(host.options.textAlpha * 100)}%</output><input aria-label="文字透明度" type="range" min="0" max="1" step="0.05" value={host.options.textAlpha} disabled={busy} onchange={(event) => void update({ textAlpha: Number(event.currentTarget.value) })} /></label>
          <label>背景透明度 <output>{Math.round(host.options.backgroundAlpha * 100)}%</output><input aria-label="背景透明度" type="range" min="0" max="1" step="0.05" value={host.options.backgroundAlpha} disabled={busy} onchange={(event) => void update({ backgroundAlpha: Number(event.currentTarget.value) })} /></label>
          <label><input type="checkbox" checked={host.options.hiddenAudioAllowed} disabled={busy} onchange={(event) => void update({ hiddenAudioAllowed: event.currentTarget.checked })} />隐身时允许声音</label>
          <label><input type="checkbox" checked={host.options.hiddenPreviewAllowed} disabled={busy} onchange={(event) => void update({ hiddenPreviewAllowed: event.currentTarget.checked })} />隐身时允许预览内容</label>
          <p class="hint">Canvas、视频和图片中的文字暂不支持单独调整；复杂网页样式需要兼容性验证。</p>
        </div>
      </details>
    {/if}
    <p class="hint" aria-live="polite">{busy ? '正在处理…' : message}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
  <section bind:this={viewport} class="viewport" aria-label="网页切片显示区域">{#if !host?.slices.length}<div class="empty"><span class="cross">＋</span><p>把关注的那一块，留在桌面</p><small>原始网页 · 独立窗口 · 等比裁剪</small></div>{/if}</section>
  <footer><span>WINDOW / {host?.options.privacyEnabled ? 'PRIVATE' : 'VISIBLE'}</span><span>Ctrl + Alt + Shift + O 恢复</span></footer>
</main>

<style>
  :global(:root) { font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif; color: #e9edf1; background: transparent; font-synthesis: none; }
  :global(*) { box-sizing: border-box; }
  :global(body) { margin: 0; }
  :global(button), :global(input) { font: inherit; }
  :global(button) { cursor: pointer; }
  :global(button:focus-visible), :global(input:focus-visible), summary:focus-visible { outline: 2px solid #9edbd2; outline-offset: 3px; }
  main { height: 100vh; display: flex; flex-direction: column; border: 1px solid #3d464c; background: transparent; overflow: hidden; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; border-bottom: 1px solid #323b42; background: #151b20; }
  button { border: 1px solid #9edbd2; background: #9edbd2; color: #102b28; border-radius: 6px; padding: 9px 12px; min-height: 36px; font-size: 13px; }
  button:hover { filter: brightness(1.12); }
  button:disabled { opacity: .5; cursor: wait; }
  button.quiet { color: #d7dfe5; background: #202930; border-color: #424e58; }
  .brand { display: flex; gap: 10px; align-items: center; border: 0; padding: 0; color: #eef4f5; background: none; text-align: left; font-size: 17px; font-weight: 600; }
  small { display: block; color: #aab8c4; font-size: 11px; font-weight: 400; margin-top: 3px; }
  .mark { display: grid; place-items: center; width: 34px; height: 34px; border: 1px solid #9edbd2; border-radius: 10px; color: #9edbd2; }
  .controls { padding: 12px 16px; background: #151b20; max-height: calc(100vh - 340px); overflow: auto; flex-shrink: 0; }
  label { display: block; font-size: 12px; color: #bac6ce; }
  .row { display: flex; gap: 8px; align-items: center; margin-top: 6px; }
  input[type=url] { min-width: 0; flex: 1; color: #edf3f5; background: #0f1519; border: 1px solid #424e58; border-radius: 6px; padding: 10px; font-size: 13px; }
  .actions { flex-wrap: wrap; margin: 10px 0; }
  details { border-top: 1px solid #323b42; padding-top: 9px; }
  summary { cursor: pointer; font-size: 12px; color: #c5d4dd; }
  .settings { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; padding-top: 12px; }
  input[type=range] { width: 100%; accent-color: #9edbd2; }
  input[type=checkbox] { accent-color: #9edbd2; vertical-align: middle; }
  output { float: right; }
  .hint, .error { font-size: 12px; line-height: 1.6; margin-bottom: 0; }
  .hint { color: #aab8c4; }
  .error { color: #ffb4b4; overflow-wrap: anywhere; }
  .viewport { flex: 1; min-height: 240px; position: relative; margin: 0 12px; background: transparent; }
  .empty { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; flex-direction: column; color: #b4c4ce; font-size: 13px; }
  .cross { font-size: 28px; color: #718795; }
  footer { display: flex; justify-content: space-between; gap: 8px; padding: 10px 14px; color: #92a4af; font-size: 10px; background: #151b20; }
</style>
