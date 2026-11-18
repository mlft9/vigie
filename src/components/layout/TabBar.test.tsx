import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { closeTab } from '@/features/terminal/TerminalSession';
import { HOME_TAB_ID, useSessions } from '@/stores/sessions';

import { TabBar } from './TabBar';

vi.mock('@/features/terminal/TerminalSession', () => ({
  closeTab: vi.fn((id: string) => {
    useSessions.getState().removeTab(id);
  }),
}));

const target = (host: string) => ({
  host,
  port: 22,
  username: 'root',
  authKind: 'password' as const,
  keyFile: null,
});

function openTabs(...hosts: string[]) {
  return hosts.map((host) => {
    const id = useSessions.getState().openTab();
    useSessions.getState().update(id, { target: target(host), status: 'connected' });
    return id;
  });
}

describe('barre d’onglets', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSessions.setState({ tabs: [], activeTabId: HOME_TAB_ID });
  });

  it('ouvre une nouvelle session avec le bouton +', async () => {
    const user = userEvent.setup();
    render(<TabBar />);
    await user.click(screen.getByRole('button', { name: /Nouvelle session/ }));
    expect(screen.getByRole('tab', { name: 'Nouvelle session' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  });

  it('affiche utilisateur@hôte et active un onglet au clic', async () => {
    const user = userEvent.setup();
    openTabs('alpha', 'beta');
    render(<TabBar />);
    await user.click(screen.getByRole('tab', { name: 'root@alpha' }));
    expect(screen.getByRole('tab', { name: 'root@alpha' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByRole('tab', { name: 'Accueil' })).toHaveAttribute('aria-selected', 'false');
  });

  it('renomme au double-clic et rétablit le titre par Échap', async () => {
    const user = userEvent.setup();
    openTabs('alpha');
    render(<TabBar />);
    await user.dblClick(screen.getByRole('tab', { name: 'root@alpha' }));
    const input = screen.getByRole('textbox', { name: "Nom de l'onglet" });
    await user.clear(input);
    await user.type(input, 'Production{Enter}');
    expect(screen.getByRole('tab', { name: 'Production' })).toBeInTheDocument();

    await user.keyboard('{F2}');
    await user.keyboard('{Escape}');
    expect(screen.getByRole('tab', { name: 'Production' })).toBeInTheDocument();
  });

  it('ferme un onglet par son bouton', async () => {
    const user = userEvent.setup();
    openTabs('alpha', 'beta');
    render(<TabBar />);
    await user.click(screen.getByRole('button', { name: "Fermer l'onglet root@alpha" }));
    expect(closeTab).toHaveBeenCalled();
    const tablist = screen.getByRole('tablist');
    expect(within(tablist).queryByRole('tab', { name: 'root@alpha' })).not.toBeInTheDocument();
  });

  it('se parcourt aux flèches du clavier', async () => {
    const user = userEvent.setup();
    openTabs('alpha');
    useSessions.getState().activate(HOME_TAB_ID);
    render(<TabBar />);
    screen.getByRole('tab', { name: 'Accueil' }).focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'root@alpha' })).toHaveFocus();
  });
});
