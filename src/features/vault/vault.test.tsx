import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import * as ipc from '@/lib/ipc';
import type * as IpcModule from '@/lib/ipc';
import { EMPTY_SNAPSHOT, useVault } from '@/stores/vault';

import { CreateVaultForm } from './CreateVaultForm';
import { characterCount, meetsPolicy } from './strength';
import { UnlockForm } from './UnlockForm';
import { TOUCH_INTERVAL_MS, useVaultSession } from './useVaultSession';
import { VaultGate } from './VaultGate';

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return {
    ...actual,
    vaultCreate: vi.fn(),
    vaultUnlock: vi.fn(),
    vaultStatus: vi.fn(),
    vaultSnapshot: vi.fn(),
    vaultWatch: vi.fn().mockResolvedValue(undefined),
    vaultTouch: vi.fn().mockResolvedValue(undefined),
  };
});

const STRONG = 'cheval agrafe batterie correct 42';

const passwordField = () => screen.getByLabelText('Mot de passe maître');

describe('politique du mot de passe (aide de l’interface)', () => {
  it('compte les caractères comme le cœur, pas les unités UTF-16', () => {
    expect(characterCount('é🔑a')).toBe(3);
    expect(meetsPolicy('court', 4)).toBe(false);
    expect(meetsPolicy('a'.repeat(12), 2)).toBe(false);
    expect(meetsPolicy(STRONG, 3)).toBe(true);
  });
});

describe('création du coffre (VLT-01)', () => {
  beforeEach(() => {
    vi.mocked(ipc.vaultCreate).mockReset();
  });

  it('prévient qu’aucune récupération n’est possible', () => {
    render(<CreateVaultForm onCreated={vi.fn()} />);
    expect(screen.getByText(/ne peut pas être récupéré/)).toBeInTheDocument();
  });

  it('refuse un mot de passe trop court ou trop faible sans rien envoyer', async () => {
    const user = userEvent.setup();
    render(<CreateVaultForm onCreated={vi.fn()} />);
    await user.type(passwordField(), 'court');
    await user.click(screen.getByRole('button', { name: 'Créer le coffre' }));
    expect(screen.getByRole('alert')).toHaveTextContent('au moins 12 caractères');

    await user.clear(passwordField());
    await user.type(passwordField(), 'passwordpassword');
    await waitFor(() => {
      expect(screen.getByTestId('password-strength')).toHaveTextContent('facile à deviner');
    });
    await user.click(screen.getByRole('button', { name: 'Créer le coffre' }));
    expect(screen.getByRole('alert')).toHaveTextContent('trop facile à deviner');
    expect(ipc.vaultCreate).not.toHaveBeenCalled();
  });

  it('exige une confirmation identique', async () => {
    const user = userEvent.setup();
    render(<CreateVaultForm onCreated={vi.fn()} />);
    await user.type(passwordField(), STRONG);
    await user.type(screen.getByLabelText('Confirmer le mot de passe'), `${STRONG}!`);
    await waitFor(() => {
      expect(screen.getByTestId('password-strength')).toHaveTextContent('robuste');
    });
    await user.click(screen.getByRole('button', { name: 'Créer le coffre' }));
    expect(screen.getByRole('alert')).toHaveTextContent('pas identiques');
    expect(ipc.vaultCreate).not.toHaveBeenCalled();
  });

  it('envoie le mot de passe une fois, puis vide les champs', async () => {
    const user = userEvent.setup();
    const onCreated = vi.fn();
    vi.mocked(ipc.vaultCreate).mockResolvedValue(undefined);
    render(<CreateVaultForm onCreated={onCreated} />);
    await user.type(passwordField(), STRONG);
    await user.type(screen.getByLabelText('Confirmer le mot de passe'), STRONG);
    await waitFor(() => {
      expect(screen.getByTestId('password-strength')).toHaveTextContent('robuste');
    });
    await user.click(screen.getByRole('button', { name: 'Créer le coffre' }));
    expect(ipc.vaultCreate).toHaveBeenCalledTimes(1);
    expect(ipc.vaultCreate).toHaveBeenCalledWith(STRONG);
    expect(passwordField()).toHaveValue('');
    expect(screen.getByLabelText('Confirmer le mot de passe')).toHaveValue('');
    expect(onCreated).toHaveBeenCalled();
  });

  it('affiche le refus du cœur, qui fait foi', async () => {
    const user = userEvent.setup();
    vi.mocked(ipc.vaultCreate).mockRejectedValue({ code: 'vault.password_too_weak' });
    render(<CreateVaultForm onCreated={vi.fn()} />);
    await user.type(passwordField(), STRONG);
    await user.type(screen.getByLabelText('Confirmer le mot de passe'), STRONG);
    await waitFor(() => {
      expect(screen.getByTestId('password-strength')).toHaveTextContent('robuste');
    });
    await user.click(screen.getByRole('button', { name: 'Créer le coffre' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('trop facile à deviner');
  });
});

describe('déverrouillage', () => {
  it('affiche un message unique en cas d’échec et vide le champ', async () => {
    const user = userEvent.setup();
    const onUnlocked = vi.fn();
    vi.mocked(ipc.vaultUnlock).mockRejectedValueOnce({
      code: 'vault.wrong_password_or_corrupted',
    });
    render(<UnlockForm reason={null} onUnlocked={onUnlocked} />);
    await user.type(passwordField(), 'mauvais');
    await user.click(screen.getByRole('button', { name: 'Déverrouiller' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Mot de passe incorrect ou coffre endommagé.',
    );
    expect(passwordField()).toHaveValue('');
    expect(onUnlocked).not.toHaveBeenCalled();

    vi.mocked(ipc.vaultUnlock).mockResolvedValueOnce(undefined);
    await user.type(passwordField(), STRONG);
    await user.click(screen.getByRole('button', { name: 'Déverrouiller' }));
    expect(onUnlocked).toHaveBeenCalled();
  });

  it('explique un verrouillage automatique', () => {
    render(<UnlockForm reason="idle" onUnlocked={vi.fn()} />);
    expect(screen.getByText(/période d'inactivité/)).toBeInTheDocument();
  });
});

describe('porte du coffre', () => {
  beforeEach(() => {
    useVault.setState({
      status: null,
      statusError: null,
      lastLockReason: null,
      snapshot: EMPTY_SNAPSHOT,
    });
  });

  it('propose la création au premier lancement', async () => {
    vi.mocked(ipc.vaultStatus).mockResolvedValue('missing');
    render(
      <VaultGate>
        <p>application</p>
      </VaultGate>,
    );
    expect(await screen.findByRole('heading', { name: 'Créer votre coffre' })).toBeInTheDocument();
    expect(screen.queryByText('application')).not.toBeInTheDocument();
  });

  it('demande le mot de passe aux lancements suivants', async () => {
    vi.mocked(ipc.vaultStatus).mockResolvedValue('locked');
    render(
      <VaultGate>
        <p>application</p>
      </VaultGate>,
    );
    expect(await screen.findByRole('heading', { name: 'Coffre verrouillé' })).toBeInTheDocument();
    expect(screen.queryByText('application')).not.toBeInTheDocument();
  });

  it('affiche l’application une fois déverrouillé, avec le contenu du coffre', async () => {
    vi.mocked(ipc.vaultStatus).mockResolvedValue('unlocked');
    vi.mocked(ipc.vaultSnapshot).mockResolvedValue({ ...EMPTY_SNAPSHOT, autoLockMinutes: 5 });
    render(
      <VaultGate>
        <p>application</p>
      </VaultGate>,
    );
    expect(await screen.findByText('application')).toBeInTheDocument();
    await waitFor(() => {
      expect(useVault.getState().snapshot.autoLockMinutes).toBe(5);
    });
  });

  it('oublie le contenu affiché au verrouillage', () => {
    useVault.setState({
      status: 'unlocked',
      snapshot: { ...EMPTY_SNAPSHOT, groups: [{ id: 'g', name: 'Lab' }] },
    });
    useVault.getState().setStatus('locked', 'idle');
    expect(useVault.getState().snapshot.groups).toHaveLength(0);
    expect(useVault.getState().lastLockReason).toBe('idle');
  });
});

describe('activité de l’utilisateur', () => {
  function Probe() {
    useVaultSession();
    return null;
  }

  it('signale l’activité au cœur au plus une fois par intervalle, coffre déverrouillé', () => {
    vi.useFakeTimers();
    vi.mocked(ipc.vaultTouch).mockClear();
    useVault.setState({ status: 'unlocked' });
    render(<Probe />);
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'b' }));
    });
    expect(ipc.vaultTouch).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(TOUCH_INTERVAL_MS + 1);
    act(() => {
      window.dispatchEvent(new PointerEvent('pointerdown'));
    });
    expect(ipc.vaultTouch).toHaveBeenCalledTimes(2);

    useVault.setState({ status: 'locked' });
    vi.advanceTimersByTime(TOUCH_INTERVAL_MS + 1);
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c' }));
    });
    expect(ipc.vaultTouch).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });
});
