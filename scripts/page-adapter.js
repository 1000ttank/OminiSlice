(() => {
  if (window.top !== window || window.__omni) return;
  const nonce = '__OMNISLICE_NONCE__';
  const post = window.chrome?.webview?.postMessage.bind(window.chrome.webview);
  const create = Object.create;
  function composition(active) {
    if (!post) return;
    // 隔离页面对 Object.prototype.toJSON 的修改，避免序列化时泄露实例 nonce。
    const payload = create(null);
    payload.nonce = nonce;
    payload.active = active;
    const message = create(null);
    message.omnisliceInteraction = payload;
    post(message);
  }
  window.addEventListener('compositionstart', event => { if (event.isTrusted) composition(true); }, true);
  window.addEventListener('compositionend', event => { if (event.isTrusted) composition(false); }, true);
  window.addEventListener('blur', event => { if (event.isTrusted) composition(false); }, true);
  const originals = new Map();
  let multipliers = [1, 1];
  let pending = false;
  let observer;
  let selecting = false;
  const colorCache = new Map();
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  const properties = ['color', 'background-color', 'border-top-color', 'border-right-color', 'border-bottom-color', 'border-left-color', 'text-decoration-color'];

  function multiply(color, factor) {
    if (factor === 1) return color;
    // 使用浏览器自身的颜色解析，保留原始 alpha；不改祖先 opacity。
    const key = `${color}/${factor}`;
    if (colorCache.has(key)) return colorCache.get(key);
    const rgb = color.match(/^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+))?\s*\)$/);
    if (rgb) {
      const result = `rgba(${rgb[1]}, ${rgb[2]}, ${rgb[3]}, ${Number(rgb[4] ?? 1) * factor})`;
      colorCache.set(key, result);
      return result;
    }
    if (!context) return color;
    context.clearRect(0, 0, 1, 1);
    context.fillStyle = color;
    context.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = context.getImageData(0, 0, 1, 1).data;
    const result = `rgba(${r}, ${g}, ${b}, ${a / 255 * factor})`;
    if (colorCache.size > 4096) colorCache.clear();
    colorCache.set(key, result);
    return result;
  }

  function apply() {
    pending = false;
    observer?.disconnect();
    for (const [element, saved] of originals) {
      if (!element.isConnected) { originals.delete(element); continue; }
      for (const [property, baseline] of saved) {
        // 网站自己更新了 inline 样式时，以网站的新值作为基线。
        if (element.style.getPropertyValue(property) !== baseline.applied || element.style.getPropertyPriority(property) !== 'important') {
          baseline.value = element.style.getPropertyValue(property);
          baseline.priority = element.style.getPropertyPriority(property);
        }
        if (baseline.value) element.style.setProperty(property, baseline.value, baseline.priority);
        else element.style.removeProperty(property);
      }
    }
    // 先采集所有节点，再统一写入，避免继承已经衰减的父级文字颜色。
    const snapshots = [];
    for (const element of document.querySelectorAll('*')) {
      if (!(element instanceof HTMLElement || element instanceof SVGElement) || element.id === 'omnislice-selector') continue;
      const computed = getComputedStyle(element);
      snapshots.push([element, properties.map(property => computed.getPropertyValue(property))]);
    }
    for (const [element, colors] of snapshots) {
      const saved = originals.get(element) || new Map();
      properties.forEach((property, index) => {
        const factor = property === 'color' || property === 'text-decoration-color' ? multipliers[0] : multipliers[1];
        if (!saved.has(property)) saved.set(property, { value: element.style.getPropertyValue(property), priority: element.style.getPropertyPriority(property), applied: '' });
        if (factor !== 1) {
          element.style.setProperty(property, multiply(colors[index], factor), 'important');
          saved.get(property).applied = element.style.getPropertyValue(property);
        } else {
          saved.delete(property);
        }
      });
      if (saved.size) originals.set(element, saved); else originals.delete(element);
    }
    observer?.observe(document.documentElement, { subtree: true, childList: true, attributes: true, attributeFilter: ['style', 'class'] });
  }
  function schedule() { if (!pending) { pending = true; requestAnimationFrame(apply); } }
  function finish(crop) {
    selecting = false;
    document.getElementById('omnislice-selector')?.remove();
    const data = JSON.stringify({ crop, scroll: { x: window.scrollX, y: window.scrollY } });
    location.href = `https://omnislice.invalid/crop?data=${encodeURIComponent(data)}`;
  }
  function select() {
    if (selecting) return;
    selecting = true;
    const host = document.createElement('div');
    host.id = 'omnislice-selector';
    host.style.cssText = 'position:fixed;inset:0;z-index:2147483647;cursor:crosshair;touch-action:none';
    const shadow = host.attachShadow({ mode: 'closed' });
    shadow.innerHTML = '<style>:host{all:initial}div{position:fixed;pointer-events:none;border:2px solid #8cf5e2;background:#8cf5e222;box-sizing:border-box}p{position:fixed;left:16px;top:8px;color:white;background:#16252e;padding:12px;font:16px sans-serif;pointer-events:none}</style><p>拖动框选 · Esc 取消</p><div></div>';
    const rectangle = shadow.querySelector('div');
    let start;
    function point(event) { return { x: Math.max(0, Math.min(innerWidth, event.clientX)), y: Math.max(0, Math.min(innerHeight, event.clientY)) }; }
    function rect(end) { return { x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), width: Math.abs(end.x - start.x), height: Math.abs(end.y - start.y) }; }
    host.onpointerdown = event => { if (event.button !== 0) return; event.preventDefault(); start = point(event); host.setPointerCapture(event.pointerId); };
    host.onpointermove = event => {
      if (!start) return;
      const value = rect(point(event));
      rectangle.style.cssText = `left:${value.x}px;top:${value.y}px;width:${value.width}px;height:${value.height}px`;
    };
    const cleanup = () => document.removeEventListener('keydown', key, true);
    const key = event => { if (event.key === 'Escape') { event.stopImmediatePropagation(); cleanup(); finish(null); } };
    host.onpointerup = event => { if (!start) return; const value = rect(point(event)); cleanup(); finish(value.width >= 1 && value.height >= 1 ? value : null); };
    host.onpointercancel = () => { cleanup(); finish(null); };
    document.addEventListener('keydown', key, true);
    document.documentElement.append(host);
  }
  window.__omni = { select, cancel() { if (selecting) finish(null); }, alpha(text, background) {
    if (![text, background].every(value => Number.isFinite(value) && value >= 0 && value <= 1)) return;
    multipliers = [text, background]; schedule();
  } };
  function ready() { observer = new MutationObserver(schedule); observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, attributeFilter: ['style', 'class'] }); }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', ready, { once: true }); else ready();
})();
