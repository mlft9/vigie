import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import * as ipc from '@/lib/ipc';
import type * as IpcModule from '@/lib/ipc';

import { HostKeyChangedAlert } from './HostKeyChangedAlert';
import { HostKeyDialog } from './HostKeyDialog';
import { QuickConnectForm } from './QuickConnectForm';

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return { ...actual, sshPickKeyFile: vi.fn() };
});

/** Champ mot de passe (le bouton radio porte le même libellé). */
const passwordField = () =>
  screen.getByLabelText('Mot de passe', { selector: 'input[type=password]' });

const KEY = { algorithm: 'ssh-ed25519', fingerprint: 'SHA256:/jxmmiM5tvPdx2D4xYj6R379gd09Gk7pM' };

describe('formulaire de connexion rapide', () => {
  it('envoie la cible et le mot de passe, puis vide le champ secret', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<QuickConnectForm initial={null} errorCode={null} onSubmit={onSubmit} />);

    await user.type(screen.getByLabelText('Hôte'), 'srv.lab');
    await user.clear(screen.getByLabelText('Port'));
    await user.type(screen.getByLabelText('Port'), '2222');
    await user.type(screen.getByLabelText('Utilisateur'), 'root');
    await user.type(passwordField(), 'hunter2');
    await user.click(screen.getByRole('button', { name: 'Se connecter' }));

    expect(onSubmit).toHaveBeenCalledWith(
      { host: 'srv.lab', port: 2222, username: 'root', authKind: 'password', keyFile: null },
      { password: 'hunter2' },
    );
    expect(passwordField()).toHaveValue('');
  });

  it('refuse un port invalide sans rien envoyer', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<QuickConnectForm initial={null} errorCode={null} onSubmit={onSubmit} />);
    await user.type(screen.getByLabelText('Hôte'), 'srv');
    await user.type(screen.getByLabelText('Utilisateur'), 'root');
    await user.clear(screen.getByLabelText('Port'));
    await user.type(screen.getByLabelText('Port'), '70000');
    await user.type(passwordField(), 'x');
    await user.click(screen.getByRole('button', { name: 'Se connecter' }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent('entre 1 et 65535');
  });

  it('choisit une clé via le cœur et ne manipule que son identifiant', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    vi.mocked(ipc.sshPickKeyFile).mockResolvedValue({ id: 3, name: 'id_ed25519' });
    render(<QuickConnectForm initial={null} errorCode={null} onSubmit={onSubmit} />);

    await user.type(screen.getByLabelText('Hôte'), 'srv');
    await user.type(screen.getByLabelText('Utilisateur'), 'root');
    await user.click(screen.getByLabelText('Clé privée'));
    await user.click(screen.getByRole('button', { name: 'Choisir une clé…' }));
    expect(await screen.findByTestId('key-file-name')).toHaveTextContent('id_ed25519');
    await user.type(screen.getByLabelText('Passphrase'), 'phrase');
    await user.click(screen.getByRole('button', { name: 'Se connecter' }));

    expect(onSubmit).toHaveBeenCalledWith(
      {
        host: 'srv',
        port: 22,
        username: 'root',
        authKind: 'key',
        keyFile: { id: 3, name: 'id_ed25519' },
      },
      { passphrase: 'phrase' },
    );
    expect(screen.getByLabelText('Passphrase')).toHaveValue('');
  });

  it('pré-remplit une reconnexion sans aucun secret et affiche la cause', () => {
    render(
      <QuickConnectForm
        initial={{ host: 'srv', port: 22, username: 'root', authKind: 'password', keyFile: null }}
        errorCode="ssh.auth_failed"
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('Hôte')).toHaveValue('srv');
    expect(passwordField()).toHaveValue('');
    expect(passwordField()).toHaveFocus();
    expect(screen.getByRole('alert')).toHaveTextContent('Authentification refusée');
  });
});

describe('clé d’hôte inconnue', () => {
  it('affiche type et empreinte, et place le focus sur « Refuser »', async () => {
    const user = userEvent.setup();
    const onDecision = vi.fn();
    render(<HostKeyDialog host="srv" port={2222} hostKey={KEY} onDecision={onDecision} />);

    expect(screen.getByRole('alertdialog', { name: 'Hôte inconnu' })).toBeInTheDocument();
    expect(screen.getByText(KEY.fingerprint)).toBeInTheDocument();
    expect(screen.getByText('ssh-ed25519')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refuser' })).toHaveFocus();

    await user.keyboard('{Escape}');
    expect(onDecision).toHaveBeenLastCalledWith(false);
    await user.click(screen.getByRole('button', { name: 'Accepter et se connecter' }));
    expect(onDecision).toHaveBeenLastCalledWith(true);
  });
});

describe('clé d’hôte modifiée', () => {
  it('bloque la connexion sans aucun moyen de continuer', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(
      <HostKeyChangedAlert
        host="srv"
        port={22}
        change={{
          key: KEY,
          known: [{ algorithm: 'ecdsa-sha2-nistp256', fingerprint: 'SHA256:ancienne' }],
          knownHostsPath: 'C:/data/known_hosts',
          hostPattern: 'srv',
        }}
        onClose={onClose}
      />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('connexion bloquée');
    expect(screen.getByText('SHA256:ancienne')).toBeInTheDocument();
    expect(screen.getByText('C:/data/known_hosts')).toBeInTheDocument();
    // Un seul bouton : fermer. Aucune option « continuer » ou « accepter ».
    const buttons = screen.getAllByRole('button');
    expect(buttons).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: "Fermer l'onglet" }));
    expect(onClose).toHaveBeenCalled();
  });
});
