import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';

import * as ipc from '@/lib/ipc';
import type * as IpcModule from '@/lib/ipc';
import type { RemoteEntry, RemoteListing } from '@/lib/types';
import { HOME_TAB_ID, type SessionTab, useSessions } from '@/stores/sessions';
import { useSftp } from '@/stores/sftp';

import { ConflictDialog } from './ConflictDialog';
import { RemoteBrowser } from './RemoteBrowser';
import { SftpSidePanel } from './SftpSidePanel';
import { TransferList } from './TransferList';

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return {
    ...actual,
    sftpOpen: vi.fn().mockResolvedValue(11),
    sftpClose: vi.fn().mockResolvedValue(undefined),
    sftpHome: vi.fn().mockResolvedValue('/home/alice'),
    sftpList: vi.fn(),
    sftpMkdir: vi.fn().mockResolvedValue(undefined),
    sftpRename: vi.fn().mockResolvedValue(undefined),
    sftpRemove: vi.fn().mockResolvedValue(undefined),
    sftpChmod: vi.fn().mockResolvedValue(undefined),
    sftpDownload: vi.fn().mockResolvedValue(1),
    sftpUpload: vi.fn().mockResolvedValue(2),
    sftpEdit: vi.fn().mockResolvedValue(3),
    sftpTransferCancel: vi.fn().mockResolvedValue(undefined),
    sftpConflictDecision: vi.fn().mockResolvedValue(undefined),
  };
});

function entry(
  name: string,
  kind: RemoteEntry['kind'],
  extra: Partial<RemoteEntry> = {},
): RemoteEntry {
  return {
    name,
    displayName: name,
    suspicious: false,
    kind,
    size: 12,
    modified: 1_700_000_000,
    permissions: kind === 'dir' ? 0o755 : 0o644,
    linkTarget: null,
    ...extra,
  };
}

function listing(path: string, entries: RemoteEntry[]): RemoteListing {
  return { path, entries, truncated: false };
}

beforeEach(() => {
  vi.clearAllMocks();
  useSftp.getState().reset();
  useSessions.setState({ tabs: [], activeTabId: HOME_TAB_ID });
  vi.mocked(ipc.sftpList).mockImplementation((_id, path) =>
    Promise.resolve(
      path === '/home/alice'
        ? listing(path, [
            entry('projets', 'dir'),
            entry('notes.txt', 'file'),
            entry('x�[2J', 'file', { name: 'x\u001b[2J', suspicious: true }),
          ])
        : listing(path, [entry('README', 'file')]),
    ),
  );
});

function Browser({ initial = null as string | null }) {
  const [path, setPath] = useState<string | null>(initial);
  return <RemoteBrowser sftpId={11} path={path} onPathChange={setPath} />;
}

describe('RemoteBrowser', () => {
  it('ouvre le dossier personnel, affiche les noms nettoyés et signale les noms suspects', async () => {
    render(<Browser />);
    expect(await screen.findByText('notes.txt')).toBeInTheDocument();
    expect(ipc.sftpHome).toHaveBeenCalledWith(11);
    expect(ipc.sftpList).toHaveBeenCalledWith(11, '/home/alice');
    // Le nom brut (avec la séquence d'échappement) n'est jamais affiché.
    expect(screen.queryByText('x\u001b[2J')).not.toBeInTheDocument();
    const suspicious = screen.getByText('x�[2J').closest('li');
    expect(suspicious && within(suspicious).getByText(/Nom inhabituel/)).toBeTruthy();
  });

  it('entre dans un dossier par double-clic et remonte au parent', async () => {
    const user = userEvent.setup();
    render(<Browser />);
    await user.dblClick(await screen.findByText('projets'));
    expect(await screen.findByText('README')).toBeInTheDocument();
    expect(ipc.sftpList).toHaveBeenLastCalledWith(11, '/home/alice/projets');
    await user.click(screen.getByRole('button', { name: 'Dossier parent' }));
    expect(await screen.findByText('notes.txt')).toBeInTheDocument();
  });

  it('supprime après confirmation, avec le nom exact du serveur', async () => {
    const user = userEvent.setup();
    render(<Browser />);
    await user.click(await screen.findByText('x�[2J'));
    await user.click(screen.getByRole('button', { name: 'Supprimer' }));
    const dialog = await screen.findByRole('alertdialog');
    // Le focus part sur « Annuler » : rien n'est supprimé sans un choix explicite.
    expect(within(dialog).getByRole('button', { name: 'Annuler' })).toHaveFocus();
    await user.click(within(dialog).getByRole('button', { name: 'Supprimer' }));
    await waitFor(() => {
      expect(ipc.sftpRemove).toHaveBeenCalledWith(11, '/home/alice/x\u001b[2J');
    });
  });

  it('modifie les droits par les cases ou la valeur octale', async () => {
    const user = userEvent.setup();
    render(<Browser />);
    await user.click(await screen.findByText('notes.txt'));
    await user.click(screen.getByRole('button', { name: 'Droits (chmod)' }));
    const octal = await screen.findByLabelText('Valeur octale');
    expect(octal).toHaveValue('644');
    await user.click(screen.getByRole('checkbox', { name: 'Propriétaire — Exécution' }));
    expect(octal).toHaveValue('744');
    await user.clear(octal);
    await user.type(octal, '600');
    await user.click(screen.getByRole('button', { name: 'Enregistrer' }));
    await waitFor(() => {
      expect(ipc.sftpChmod).toHaveBeenCalledWith(11, '/home/alice/notes.txt', 0o600);
    });
  });

  it('télécharge la sélection via la boîte de dialogue du système', async () => {
    const user = userEvent.setup();
    render(<Browser />);
    await user.click(await screen.findByText('notes.txt'));
    await user.click(screen.getByRole('button', { name: 'Télécharger…' }));
    expect(ipc.sftpDownload).toHaveBeenCalledWith(11, ['/home/alice/notes.txt'], { kind: 'pick' });
  });

  it('refuse un chemin relatif saisi dans la barre d’adresse', async () => {
    const user = userEvent.setup();
    render(<Browser />);
    const address = await screen.findByLabelText('Chemin distant');
    await waitFor(() => {
      expect(address).toHaveValue('/home/alice');
    });
    await user.clear(address);
    await user.type(address, 'etc{Enter}');
    expect(await screen.findByRole('alert')).toHaveTextContent('doit commencer par « / »');
  });
});

describe('ConflictDialog et TransferList', () => {
  function queue() {
    const { handleEvent } = useSftp.getState();
    handleEvent(11, {
      type: 'transferQueued',
      job: 4,
      direction: 'download',
      label: 'gros.iso',
      count: 1,
    });
    handleEvent(11, {
      type: 'transferProgress',
      job: 4,
      bytesDone: 512,
      bytesTotal: 1024,
      filesDone: 0,
      filesTotal: 1,
    });
  }

  it('affiche la progression et annule un transfert', async () => {
    const user = userEvent.setup();
    queue();
    render(<TransferList sftpId={11} />);
    expect(screen.getByRole('progressbar', { name: 'Progression de gros.iso' })).toHaveAttribute(
      'value',
      '512',
    );
    await user.click(screen.getByRole('button', { name: 'Annuler le transfert' }));
    expect(ipc.sftpTransferCancel).toHaveBeenCalledWith(11, 4);
  });

  it('pose la question de conflit, « Ignorer » par défaut', async () => {
    const user = userEvent.setup();
    queue();
    act(() => {
      useSftp.getState().handleEvent(11, { type: 'transferConflict', job: 4, name: 'gros.iso' });
    });
    const transfer = useSftp.getState().transfers[0];
    if (!transfer) throw new Error('transfert attendu');
    render(<ConflictDialog transfer={transfer} />);
    expect(screen.getByRole('button', { name: 'Ignorer' })).toHaveFocus();
    await user.click(screen.getByRole('checkbox', { name: /Appliquer aux conflits suivants/ }));
    await user.click(screen.getByRole('button', { name: 'Garder les deux' }));
    expect(ipc.sftpConflictDecision).toHaveBeenCalledWith(11, 4, 'rename', true);
  });
});

describe('SftpSidePanel', () => {
  function terminalTab(patch: Partial<SessionTab>): SessionTab {
    const id = useSessions.getState().openTab();
    useSessions.getState().update(id, { status: 'connected', sessionId: 7, ...patch });
    const tab = useSessions.getState().tabs.find((t) => t.id === id);
    if (!tab) throw new Error('onglet attendu');
    return tab;
  }

  it('ouvre le SFTP sur la session et suit le dossier du shell', async () => {
    const tab = terminalTab({ remoteCwd: '/srv' });
    const { rerender } = render(<SftpSidePanel tab={tab} />);
    expect(await screen.findByText('README')).toBeInTheDocument();
    expect(ipc.sftpOpen).toHaveBeenCalledWith(7, expect.any(Function));
    expect(ipc.sftpList).toHaveBeenLastCalledWith(11, '/srv');

    rerender(<SftpSidePanel tab={{ ...tab, remoteCwd: '/home/alice' }} />);
    expect(await screen.findByText('notes.txt')).toBeInTheDocument();

    // Suivi désactivé : un nouveau `cd` ne change plus le dossier affiché.
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Suivre le terminal' }));
    rerender(<SftpSidePanel tab={{ ...tab, remoteCwd: '/srv' }} />);
    await waitFor(() => {
      expect(ipc.sftpList).toHaveBeenLastCalledWith(11, '/home/alice');
    });
  });

  it('propose l’intégration au prompt quand le shell n’annonce pas son dossier', async () => {
    const tab = terminalTab({ remoteCwd: null });
    render(<SftpSidePanel tab={tab} />);
    expect(await screen.findByRole('button', { name: /Activer le suivi/ })).toBeInTheDocument();
  });

  it('ferme le client SFTP au démontage', async () => {
    const tab = terminalTab({ remoteCwd: '/srv' });
    const { unmount } = render(<SftpSidePanel tab={tab} />);
    await screen.findByText('README');
    unmount();
    expect(ipc.sftpClose).toHaveBeenCalledWith(11);
  });
});
