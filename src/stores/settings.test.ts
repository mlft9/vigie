import { SETTINGS_STORAGE_KEY, useSettings } from './settings';

describe('useSettings', () => {
  beforeEach(() => {
    useSettings.setState({ theme: 'system', language: 'system' });
  });

  it('suit le système par défaut', () => {
    const { theme, language } = useSettings.getState();
    expect(theme).toBe('system');
    expect(language).toBe('system');
  });

  it('mémorise uniquement le thème et la langue', () => {
    useSettings.getState().setTheme('dark');
    useSettings.getState().setLanguage('en');
    const stored = JSON.parse(localStorage.getItem(SETTINGS_STORAGE_KEY) ?? '{}') as {
      state: Record<string, unknown>;
    };
    expect(stored.state).toEqual({ theme: 'dark', language: 'en' });
  });

  it('ignore des valeurs invalides dans le stockage local', async () => {
    localStorage.setItem(
      SETTINGS_STORAGE_KEY,
      JSON.stringify({ state: { theme: 'neon', language: '<script>' }, version: 1 }),
    );
    await useSettings.persist.rehydrate();
    expect(useSettings.getState().theme).toBe('system');
    expect(useSettings.getState().language).toBe('system');
  });

  it('relit des valeurs valides', async () => {
    localStorage.setItem(
      SETTINGS_STORAGE_KEY,
      JSON.stringify({ state: { theme: 'light', language: 'fr' }, version: 1 }),
    );
    await useSettings.persist.rehydrate();
    expect(useSettings.getState().theme).toBe('light');
    expect(useSettings.getState().language).toBe('fr');
  });
});
