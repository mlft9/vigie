import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import * as ipc from '@/lib/ipc';
import type * as IpcModule from '@/lib/ipc';
import type { IdentityEntry, KeyEntry, KnownHostEntry } from '@/lib/types';
import { EMPTY_SNAPSHOT, useVault } from '@/stores/vault';

import { IdentityForm } from './IdentityForm';
import { KeyImportDialog } from './KeyImportDialog';
import { KeysView } from './KeysView';
import { KnownHostsList } from './KnownHostsList';

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return {
    ...actual,
    identitySave: vi.fn().mockResolvedValue('i-new'),
    identityDelete: vi.fn().mockResolvedValue(undefined),
    keyImport: vi.fn(),
    keyDelete: vi.fn().mockResolvedValue(undefined),
    keyRename: vi.fn().mockResolvedValue(undefined),
    sshPickKeyFile: vi.fn(),
    knownHostList: vi.fn(),
    knownHostDelete: vi.fn().mockResolvedValue(1),
    vaultSnapshot: vi.fn(),
  };
});

const KEY: KeyEntry = {
  id: 'k-1',
  name: 'lab',
  algorithm: 'ssh-ed25519',
  fingerprint: 'SHA256:abc',
  publicKey: 'ssh-ed25519 AAAAC3Nza',
};
const IDENTITY: IdentityEntry = {
  id: 'i-1',
  name: 'Admin',
  username: 'admin',
  hasPassword: true,
  keyId: null,
};
const KNOWN: KnownHostEntry = {
  hosts: '[srv.lab]:2222',
  algorithm: 'ssh-ed25519',
  fingerprint: 'SHA256:ancienne',
  revoked: false,
};

beforeEach(() => {
  vi.mocked(ipc.identitySave).mockClear();
  vi.mocked(ipc.knownHostList).mockResolvedValue([KNOWN]);
  vi.mocked(ipc.knownHostDelete).mockClear();
});

describe('identités (HST-03)', () => {
  it('garde le mot de passe enregistré quand le champ reste vide', async () => {
    const user = userEvent.setup();
    render(<IdentityForm identity={IDENTITY} keys={[KEY]} onSaved={vi.fn()} onClose={vi.fn()} />);
    await user.click(screen.getByRole('button', { name: 'Enregistrer' }));
    expect(ipc.identitySave).toHaveBeenCalledWith({
      id: 'i-1',
      name: 'Admin',
      username: 'admin',
      password: { action: 'keep' },
      keyId: null,
    });
  });

  it('remplace le mot de passe, puis vide le champ', async () => {
    const user = userEvent.setup();
    render(<IdentityForm identity={IDENTITY} keys={[KEY]} onSaved={vi.fn()} onClose={vi.fn()} />);
    const field = screen.getByLabelText('Nouveau mot de passe');
    await user.type(field, 'nouveau');
    await user.click(screen.getByRole('button', { name: 'Enregistrer' }));
    expect(ipc.identitySave).toHaveBeenCalledWith(
      expect.objectContaining({ password: { action: 'set', value: 'nouveau' } }),
    );
    expect(field).toHaveValue('');
  });

  it('retire le mot de passe seulement si une clé reste', async () => {
    const user = userEvent.setup();
    render(<IdentityForm identity={IDENTITY} keys={[KEY]} onSaved={vi.fn()} onClose={vi.fn()} />);
    await user.click(screen.getByLabelText('Retirer le mot de passe enregistré'));
    await user.click(screen.getByRole('button', { name: 'Enregistrer' }));
    expect(screen.getByRole('alert')).toHaveTextContent('mot de passe, une clé, ou les deux');
    expect(ipc.identitySave).not.toHaveBeenCalled();

    await user.selectOptions(screen.getByLabelText('Clé'), 'k-1');
    await user.click(screen.getByRole('button', { name: 'Enregistrer' }));
    expect(ipc.identitySave).toHaveBeenCalledWith(
      expect.objectContaining({ password: { action: 'clear' }, keyId: 'k-1' }),
    );
  });
});

describe('import de clé (KEY-02)', () => {
  it('choisit le fichier par le cœur et n’envoie que son identifiant', async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.sshPickKeyFile).mockResolvedValue({ id: 5, name: 'id_ed25519' });
    vi.mocked(ipc.keyImport).mockResolvedValue(KEY);
    const onImported = vi.fn();
    render(<KeyImportDialog onImported={onImported} onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Importer' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Choisir une clé…' }));
    expect(screen.getByTestId('import-file-name')).toHaveTextContent('id_ed25519');
    expect(screen.getByLabelText('Nom')).toHaveValue('id_ed25519');
    await user.click(screen.getByRole('button', { name: 'Importer' }));
    expect(ipc.keyImport).toHaveBeenCalledWith({
      keyFileId: 5,
      name: 'id_ed25519',
      passphrase: null,
    });
    expect(onImported).toHaveBeenCalled();
  });

  it('transmet la passphrase une fois, puis vide le champ', async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.sshPickKeyFile).mockResolvedValue({ id: 5, name: 'id_ed25519' });
    vi.mocked(ipc.keyImport).mockRejectedValueOnce({ code: 'ssh.key_wrong_passphrase' });
    render(<KeyImportDialog onImported={vi.fn()} onClose={vi.fn()} />);
    await user.click(screen.getByRole('button', { name: 'Choisir une clé…' }));
    await user.type(screen.getByLabelText('Passphrase'), 'mauvaise');
    await user.click(screen.getByRole('button', { name: 'Importer' }));
    expect(ipc.keyImport).toHaveBeenCalledWith(expect.objectContaining({ passphrase: 'mauvaise' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Passphrase incorrecte');
    expect(screen.getByLabelText('Passphrase')).toHaveValue('');
  });
});

describe('clés d’hôte connues (HKY-04)', () => {
  it('ne supprime une clé qu’après un avertissement explicite', async () => {
    const user = userEvent.setup();
    render(<KnownHostsList />);
    const item = await screen.findByText('[srv.lab]:2222');
    expect(item).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Supprimer la clé de [srv.lab]:2222' }));
    const dialog = screen.getByRole('alertdialog');
    expect(dialog).toHaveTextContent('SHA256:ancienne');
    expect(dialog).toHaveTextContent('vérifié auprès de l');
    // Le focus part sur « Annuler » : une validation par Entrée ne supprime rien.
    expect(within(dialog).getByRole('button', { name: 'Annuler' })).toHaveFocus();
    await user.click(within(dialog).getByRole('button', { name: 'Supprimer la clé' }));
    expect(ipc.knownHostDelete).toHaveBeenCalledWith(KNOWN);
  });

  it('signale une liste vide', async () => {
    vi.mocked(ipc.knownHostList).mockResolvedValue([]);
    render(<KnownHostsList />);
    expect(await screen.findByText("Aucune clé d'hôte enregistrée.")).toBeInTheDocument();
  });
});

describe('section Clés', () => {
  it('affiche clés, identités et clés d’hôte, sans aucun secret', async () => {
    useVault.setState({
      status: 'unlocked',
      snapshot: { ...EMPTY_SNAPSHOT, keys: [KEY], identities: [{ ...IDENTITY, keyId: 'k-1' }] },
    });
    render(<KeysView />);
    expect(screen.getByText('SHA256:abc', { exact: false })).toBeInTheDocument();
    expect(screen.getByText('admin · clé lab + mot de passe')).toBeInTheDocument();
    expect(await screen.findByText('[srv.lab]:2222')).toBeInTheDocument();
  });

  it('refuse la suppression d’une clé utilisée (message du cœur)', async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.keyDelete).mockRejectedValueOnce({ code: 'vault.key_in_use' });
    vi.mocked(ipc.vaultSnapshot).mockResolvedValue({ ...EMPTY_SNAPSHOT, keys: [KEY] });
    useVault.setState({ status: 'unlocked', snapshot: { ...EMPTY_SNAPSHOT, keys: [KEY] } });
    render(<KeysView />);
    await user.click(screen.getByRole('button', { name: 'Supprimer lab' }));
    await user.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Supprimer' }),
    );
    expect(await screen.findByText(/utilisée par une identité/)).toBeInTheDocument();
  });
});
