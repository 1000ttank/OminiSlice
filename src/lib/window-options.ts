import type { WindowId, WindowMode, WindowOptions } from './types';

export function defaultWindowOptions(mode: WindowMode): WindowOptions {
  return {
    privacyEnabled: mode === 'privacy', manualHidden: false, manualPassthrough: false,
    hiddenResidualAlpha: 0, textAlpha: 1, backgroundAlpha: 1, topmost: false,
    hiddenPreviewAllowed: false, hiddenAudioAllowed: false,
  };
}

export function validateWindowOptions(input: unknown, knownIds: readonly WindowId[]): {
  windowId: WindowId; options: WindowOptions;
} {
  if (typeof input !== 'object' || input === null) throw new Error('窗口设置必须是对象');
  const request = input as Record<string, unknown>;
  if (typeof request.windowId !== 'string' || !knownIds.some(id => id === request.windowId)) {
    throw new Error('未知窗口 ID');
  }
  if (typeof request.options !== 'object' || request.options === null) throw new Error('缺少窗口设置');
  const options = request.options as Record<string, unknown>;
  const booleanKeys = ['privacyEnabled', 'manualHidden', 'manualPassthrough', 'topmost',
    'hiddenPreviewAllowed', 'hiddenAudioAllowed'] as const;
  const alphaKeys = ['hiddenResidualAlpha', 'textAlpha', 'backgroundAlpha'] as const;
  for (const key of booleanKeys) {
    if (typeof options[key] !== 'boolean') throw new Error(`${key} 必须为布尔值`);
  }
  for (const key of alphaKeys) {
    const value = options[key];
    if (typeof value !== 'number' || !Number.isFinite(value) || value < 0 || value > 1) {
      throw new Error(`${key} 必须是 0 到 1 之间的有限数值`);
    }
  }
  if (Object.keys(options).some(key => ![...booleanKeys, ...alphaKeys].includes(key as typeof booleanKeys[number]))) {
    throw new Error('包含未知窗口设置');
  }
  return {
    windowId: request.windowId as WindowId,
    options: {
      privacyEnabled: options.privacyEnabled as boolean, manualHidden: options.manualHidden as boolean,
      manualPassthrough: options.manualPassthrough as boolean, topmost: options.topmost as boolean,
      hiddenPreviewAllowed: options.hiddenPreviewAllowed as boolean, hiddenAudioAllowed: options.hiddenAudioAllowed as boolean,
      hiddenResidualAlpha: options.hiddenResidualAlpha as number, textAlpha: options.textAlpha as number,
      backgroundAlpha: options.backgroundAlpha as number,
    },
  };
}
