export type WindowId = `window-${string}`;
export type WebviewId = `control-${WindowId}` | `content-${string}`;
export type SliceId = `slice-${string}`;
export type WindowMode = 'privacy' | 'always-visible';

export interface WindowOptions {
  privacyEnabled: boolean;
  manualHidden: boolean;
  manualPassthrough: boolean;
  hiddenResidualAlpha: number;
  textAlpha: number;
  backgroundAlpha: number;
  topmost: boolean;
  hiddenPreviewAllowed: boolean;
  hiddenAudioAllowed: boolean;
}

export interface Slice {
  id: SliceId;
  url: string;
  layoutViewport: { width: number; height: number };
  crop: { x: number; y: number; width: number; height: number };
  scroll: { x: number; y: number };
  zoom: number;
}

export interface HostWindow {
  id: WindowId;
  controlWebviewId: WebviewId;
  options: WindowOptions;
  revision: number;
  slices: Slice[];
  display: 'visible' | 'privacy-hidden' | 'manual-hidden';
  interaction: 'interactive' | 'passthrough';
  protections: string[];
}
