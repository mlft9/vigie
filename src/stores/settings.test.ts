import * as ipc from '@/lib/ipc';
import type * as IpcModule from '@/lib/ipc';

import { LEGACY_STORAGE_KEY, loadSettings, useSettings } from './settings';

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return {
    ...actual,
    settingsGet: vi.fn(),
    settingsSet: vi.fn().mockResolvedValue(undefined),
  };
});

const DEFAULTS = { theme: 'system', language: 'system', terminalFontSize: 14 } as const;

describe('useSettings', () => {
  beforeEach(() => {
    useSettings.setState(DEFAULTS);
    vi.mocked(ipc.settingsSet).mockClear();
  });

  it('suit le système par défaut', () => {
    const { theme, language, terminalFontSize } = useSettings.getState();
    expect(theme).toBe('system');
    expect(language).toBe('system');
    expect(terminalFontSize).toBe(14);
  });

  it('enregistre chaque changement dans le cœur (settings.json), pas dans localStorage', () => {
    useSettings.getState().setTheme('dark');
    useSettings.getState().setLanguage('en');
    useSettings.getState().setTerminalFontSize(16);
    expect(ipc.settingsSet).toHaveBeenLastCalledWith({
      theme: 'dark',
      language: 'en',
      terminalFontSize: 16,
    });
    expect(localStorage.length).toBe(0);
  });

  it('refuse une taille de police hors bornes', () => {
    useSettings.getState().setTerminalFontSize(200);
    expect(useSettings.getState().terminalFontSize).toBe(14);
    expect(ipc.settingsSet).not.toHaveBeenCalled();
  });

  it('charge les paramètres du cœur en ignorant les valeurs invalides', async () => {
    vi.mocked(ipc.settingsGet).mockResolvedValueOnce({
      settings: {
        theme: 'light',
        // Valeur inattendue : un cœur ou un fichier altéré ne doit rien injecter.
        language: '<script>' as unknown as 'fr',
        terminalFontSize: 18,
      },
      firstRun: false,
    });
    await loadSettings();
    expect(useSettings.getState()).toMatchObject({
      theme: 'light',
      language: 'system',
      terminalFontSize: 18,
    });
  });

  it('reprend une fois les anciennes préférences du localStorage, puis les efface', async () => {
    localStorage.setItem(
      LEGACY_STORAGE_KEY,
      JSON.stringify({ state: { theme: 'dark', language: 'fr' }, version: 1 }),
    );
    vi.mocked(ipc.settingsGet).mockResolvedValueOnce({
      settings: { ...DEFAULTS },
      firstRun: true,
    });
    await loadSettings();
    expect(useSettings.getState()).toMatchObject({ theme: 'dark', language: 'fr' });
    expect(ipc.settingsSet).toHaveBeenCalledWith({
      theme: 'dark',
      language: 'fr',
      terminalFontSize: 14,
    });
    expect(localStorage.getItem(LEGACY_STORAGE_KEY)).toBeNull();
  });

  it('garde les valeurs par défaut si le cœur est indisponible', async () => {
    vi.mocked(ipc.settingsGet).mockRejectedValueOnce({ code: 'unknown' });
    await loadSettings();
    expect(useSettings.getState()).toMatchObject(DEFAULTS);
  });
});
