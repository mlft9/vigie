import type { ITheme } from '@xterm/xterm';

import type { ResolvedTheme } from '@/features/settings/theme';

/** Police à chasse fixe disponible sur Windows et sur les distributions Linux courantes. */
export const TERMINAL_FONT_FAMILY =
  "'Cascadia Mono', 'Cascadia Code', Consolas, 'DejaVu Sans Mono', 'Liberation Mono', 'Ubuntu Mono', monospace";

const DARK: ITheme = {
  background: '#16161a',
  foreground: '#e4e4e7',
  cursor: '#e4e4e7',
  cursorAccent: '#16161a',
  selectionBackground: '#3f3f46',
  black: '#27272a',
  red: '#f87171',
  green: '#4ade80',
  yellow: '#facc15',
  blue: '#60a5fa',
  magenta: '#c084fc',
  cyan: '#22d3ee',
  white: '#d4d4d8',
  brightBlack: '#71717a',
  brightRed: '#fca5a5',
  brightGreen: '#86efac',
  brightYellow: '#fde047',
  brightBlue: '#93c5fd',
  brightMagenta: '#d8b4fe',
  brightCyan: '#67e8f9',
  brightWhite: '#fafafa',
};

const LIGHT: ITheme = {
  background: '#fafafa',
  foreground: '#27272a',
  cursor: '#27272a',
  cursorAccent: '#fafafa',
  selectionBackground: '#d4d4d8',
  black: '#27272a',
  red: '#dc2626',
  green: '#16a34a',
  yellow: '#a16207',
  blue: '#2563eb',
  magenta: '#9333ea',
  cyan: '#0e7490',
  white: '#a1a1aa',
  brightBlack: '#52525b',
  brightRed: '#ef4444',
  brightGreen: '#22c55e',
  brightYellow: '#ca8a04',
  brightBlue: '#3b82f6',
  brightMagenta: '#a855f7',
  brightCyan: '#0891b2',
  brightWhite: '#18181b',
};

export function terminalTheme(theme: ResolvedTheme): ITheme {
  return theme === 'dark' ? DARK : LIGHT;
}
