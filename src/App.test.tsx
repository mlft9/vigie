import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { App } from './App';
import { i18n } from './i18n';
import * as ipc from './lib/ipc';
import type * as IpcModule from './lib/ipc';
import { useSettings } from './stores/settings';
import { useUi } from './stores/ui';

vi.mock('./lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return { ...actual, appInfo: vi.fn() };
});

function setup() {
  vi.mocked(ipc.appInfo).mockResolvedValue({ name: 'Vigie', version: '0.1.0' });
  return { user: userEvent.setup(), ...render(<App />) };
}

describe('App', () => {
  beforeEach(async () => {
    useSettings.setState({ theme: 'system', language: 'fr' });
    useUi.setState({ activeSection: 'hosts' });
    await i18n.changeLanguage('fr');
  });

  it('affiche la barre latérale avec toutes les sections (APP-01)', () => {
    setup();
    const nav = screen.getByRole('navigation', { name: 'Navigation principale' });
    for (const label of ['Hôtes', 'Clés', 'Snippets', 'Tunnels', 'SFTP', 'Paramètres']) {
      expect(within(nav).getByRole('button', { name: label })).toBeInTheDocument();
    }
    expect(screen.getByRole('tab', { name: 'Accueil' })).toHaveAttribute('aria-selected', 'true');
  });

  it('change de section au clic', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Tunnels' }));
    expect(screen.getByRole('heading', { name: 'Tunnels' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Tunnels' })).toHaveAttribute('aria-current', 'page');
  });

  it('bascule la langue à chaud (APP-02)', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Paramètres' }));
    await user.selectOptions(screen.getByLabelText('Langue'), 'en');
    expect(await screen.findByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    expect(document.documentElement.lang).toBe('en');

    await user.selectOptions(screen.getByLabelText('Language'), 'fr');
    expect(await screen.findByRole('heading', { name: 'Paramètres' })).toBeInTheDocument();
  });

  it('force le thème puis revient au système (APP-03)', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Paramètres' }));
    const select = screen.getByLabelText('Thème');

    await user.selectOptions(select, 'dark');
    expect(document.documentElement).toHaveClass('dark');

    await user.selectOptions(select, 'light');
    expect(document.documentElement).toHaveClass('light');
    expect(document.documentElement).not.toHaveClass('dark');

    await user.selectOptions(select, 'system');
    // jsdom n'implémente pas matchMedia : le système est considéré comme clair.
    expect(document.documentElement).toHaveClass('light');
  });

  it('affiche la version renvoyée par le backend', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Paramètres' }));
    expect(await screen.findByTestId('app-version')).toHaveTextContent('0.1.0');
  });

  it('reste utilisable si le backend est indisponible', async () => {
    vi.mocked(ipc.appInfo).mockRejectedValue({ code: 'unknown' });
    render(<App />);
    await act(async () => {
      useUi.setState({ activeSection: 'settings' });
      await Promise.resolve();
    });
    expect(screen.getByTestId('app-version')).toHaveTextContent('indisponible');
  });
});
