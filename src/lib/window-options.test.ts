import { describe, expect, it } from 'vitest';
import { defaultWindowOptions, validateWindowOptions } from './window-options';
import type { WindowId } from './types';

const ids: WindowId[] = ['window-001', 'window-002'];
function request() { return { windowId: ids[0], options: defaultWindowOptions('privacy') }; }

describe('真实控制界面使用的 WindowOptions 验证', () => {
  for (const alpha of ['textAlpha', 'backgroundAlpha', 'hiddenResidualAlpha'] as const) {
    it.each([NaN, Infinity, -Infinity, -0.1, 1.1])(`拒绝 ${alpha}=%s`, value => {
      const input = request(); input.options[alpha] = value;
      expect(() => validateWindowOptions(input, ids)).toThrow();
    });
    it.each([0, 1])(`接受 ${alpha} 边界 %s`, value => {
      const input = request(); input.options[alpha] = value;
      expect(validateWindowOptions(input, ids).options[alpha]).toBe(value);
    });
  }
  it.each(['privacyEnabled', 'manualHidden', 'manualPassthrough', 'topmost', 'hiddenPreviewAllowed', 'hiddenAudioAllowed'])(
    '拒绝 %s 的非布尔值', key => {
      const input = request();
      expect(() => validateWindowOptions({ ...input, options: { ...input.options, [key]: 'false' } }, ids)).toThrow();
    });
  it.each([null, false, {}, { windowId: 'window-999', options: defaultWindowOptions('privacy') }])('拒绝无效请求 %j', input => {
    expect(() => validateWindowOptions(input, ids)).toThrow();
  });
  it('A 的 patch 不改变 B，验证结果不共享输入对象', () => {
    const a = request(); const b = defaultWindowOptions('always-visible');
    a.options.textAlpha = 0.5;
    const result = validateWindowOptions(a, ids);
    result.options.backgroundAlpha = 0.25;
    expect(b).toEqual(defaultWindowOptions('always-visible'));
    expect(a.options.backgroundAlpha).toBe(1);
  });
  it('拒绝未知字段', () => {
    const input = request();
    expect(() => validateWindowOptions({ ...input, options: { ...input.options, arbitrary: true } }, ids)).toThrow();
  });
});
