import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import type * as IpcModule from '@/lib/ipc';
import type { AuthPromptInfo } from '@/lib/types';

import { AuthPromptDialog } from './AuthPromptDialog';
import { QuickConnectForm } from './QuickConnectForm';

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return { ...actual, sshPickKeyFile: vi.fn() };
});

const PAM: AuthPromptInfo = {
  name: '',
  instructions: '',
  prompts: [{ text: 'Verification code: ', echo: false }],
};

describe('questions du serveur (AUTH-03)', () => {
  it('envoie les réponses puis vide les champs', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <AuthPromptDialog address="srv.lab" prompt={PAM} onSubmit={onSubmit} onCancel={vi.fn()} />,
    );
    const field = screen.getByLabelText('Verification code:');
    expect(field).toHaveAttribute('type', 'password');
    expect(field).toHaveFocus();
    await user.type(field, '123456');
    await user.click(screen.getByRole('button', { name: 'Valider' }));
    expect(onSubmit).toHaveBeenCalledWith(['123456']);
    expect(field).toHaveValue('');
  });

  it('affiche le texte du serveur comme du texte brut, distingué de l’interface', () => {
    render(
      <AuthPromptDialog
        address="srv.lab:2222"
        prompt={{
          name: '<b>PAM</b>',
          instructions: 'Ligne 1\nLigne 2 <img src=x onerror=alert(1)>',
          prompts: [
            { text: 'Utilisateur :', echo: true },
            { text: '', echo: false },
          ],
        }}
        onSubmit={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('<b>PAM</b>')).toBeInTheDocument();
    expect(document.querySelector('b, img')).toBeNull();
    expect(screen.getByText('Message envoyé par le serveur')).toBeInTheDocument();
    expect(screen.getByText(/Le serveur srv\.lab:2222 demande/)).toBeInTheDocument();
    // Écho autorisé par le serveur : champ texte ; question vide : libellé de repli.
    expect(screen.getByLabelText('Utilisateur :')).toHaveAttribute('type', 'text');
    expect(screen.getByLabelText('Réponse 2')).toHaveAttribute('type', 'password');
  });

  it('rappelle que Younji ne demande jamais le mot de passe maître ici', () => {
    render(<AuthPromptDialog address="srv" prompt={PAM} onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByText(/jamais votre mot de passe maître/)).toBeInTheDocument();
  });

  it('annule par le bouton ou par Échap', async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    const onSubmit = vi.fn();
    render(<AuthPromptDialog address="srv" prompt={PAM} onSubmit={onSubmit} onCancel={onCancel} />);
    await user.click(screen.getByRole('button', { name: 'Annuler' }));
    await user.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});

describe('connexion rapide en mode interactif', () => {
  it('n’envoie aucun secret et laisse le serveur poser ses questions', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<QuickConnectForm initial={null} errorCode={null} onSubmit={onSubmit} />);
    await user.type(screen.getByLabelText('Hôte'), 'srv.lab');
    await user.type(screen.getByLabelText('Utilisateur'), 'test2fa');
    await user.click(screen.getByLabelText('Questions du serveur (2FA)'));
    expect(screen.queryByLabelText('Passphrase')).toBeNull();
    expect(document.querySelector('input[type=password]')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Se connecter' }));
    expect(onSubmit).toHaveBeenCalledWith(
      { host: 'srv.lab', port: 22, username: 'test2fa', authKind: 'interactive', keyFile: null },
      {},
    );
  });
});
