import { applyTheme, resolveTheme, watchSystemTheme } from './theme';

describe('resolveTheme', () => {
  it('suit le système en mode « system »', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });

  it('ignore le système quand le thème est forcé', () => {
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });
});

describe('applyTheme', () => {
  it('pose une seule classe de thème sur la racine', () => {
    const root = document.createElement('html');
    applyTheme('dark', root);
    expect(root.classList.contains('dark')).toBe(true);
    expect(root.classList.contains('light')).toBe(false);
    applyTheme('light', root);
    expect(root.classList.contains('dark')).toBe(false);
    expect(root.classList.contains('light')).toBe(true);
  });
});

describe('watchSystemTheme', () => {
  it('relaie les changements du système et se désabonne', () => {
    let listener: ((e: { matches: boolean }) => void) | undefined;
    const removeEventListener = vi.fn();
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => ({
        matches: false,
        addEventListener: (_: string, l: (e: { matches: boolean }) => void) => {
          listener = l;
        },
        removeEventListener,
      })),
    );

    const onChange = vi.fn();
    const stop = watchSystemTheme(onChange);
    listener?.({ matches: true });
    expect(onChange).toHaveBeenCalledWith(true);

    stop();
    expect(removeEventListener).toHaveBeenCalled();
    vi.unstubAllGlobals();
  });
});
