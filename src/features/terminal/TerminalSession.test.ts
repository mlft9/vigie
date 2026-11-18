import * as ipc from '@/lib/ipc';
import type * as IpcModule from '@/lib/ipc';
import type { SessionEvent } from '@/lib/types';
import { type ConnectTarget, HOME_TAB_ID, useSessions } from '@/stores/sessions';
import { useSettings } from '@/stores/settings';

import { closeAllTabs, terminalSession, TerminalSession } from './TerminalSession';

/** Accès au double de xterm, dont les méthodes sont des fonctions simulées. */
function fake(session: TerminalSession) {
  return session.terminal as unknown as {
    paste: ReturnType<typeof vi.fn>;
    dispose: ReturnType<typeof vi.fn>;
    written: Uint8Array[];
  };
}

// xterm.js a besoin d'un vrai moteur de rendu : on le remplace par un double minimal.
vi.mock('@xterm/xterm', () => {
  class FakeTerminal {
    cols = 80;
    rows = 24;
    options: Record<string, unknown> = {};
    unicode = { activeVersion: '' };
    written: Uint8Array[] = [];
    paste = vi.fn();
    focus = vi.fn();
    dispose = vi.fn();
    clearSelection = vi.fn();
    loadAddon = vi.fn();
    open = vi.fn();
    getSelection = vi.fn(() => '');
    hasSelection = vi.fn(() => false);
    onData = vi.fn();
    onBinary = vi.fn();
    onResize = vi.fn();
    write(data: Uint8Array, callback?: () => void) {
      this.written.push(data);
      callback?.();
    }
  }
  return { Terminal: FakeTerminal };
});
vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    dispose = vi.fn();
  },
}));
vi.mock('@xterm/addon-search', () => ({
  SearchAddon: class {
    dispose = vi.fn();
  },
}));
vi.mock('@xterm/addon-unicode11', () => ({
  Unicode11Addon: class {
    dispose = vi.fn();
  },
}));
vi.mock('@xterm/addon-web-links', () => ({
  WebLinksAddon: class {
    dispose = vi.fn();
  },
}));
vi.mock('@xterm/addon-webgl', () => ({
  WebglAddon: class {
    dispose = vi.fn();
  },
}));

vi.mock('@/lib/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof IpcModule>();
  return {
    ...actual,
    sshConnect: vi.fn(),
    sshConnectHost: vi.fn(),
    sshWrite: vi.fn().mockResolvedValue(undefined),
    sshResize: vi.fn().mockResolvedValue(undefined),
    sshAck: vi.fn().mockResolvedValue(undefined),
    sshClose: vi.fn().mockResolvedValue(undefined),
    sshHostKeyDecision: vi.fn().mockResolvedValue(undefined),
    sshAuthAnswer: vi.fn().mockResolvedValue(undefined),
    openExternalUrl: vi.fn().mockResolvedValue(undefined),
  };
});

const TARGET: ConnectTarget = {
  host: 'srv.lab',
  port: 22,
  username: 'root',
  authKind: 'password',
  keyFile: null,
};
const KEY = { algorithm: 'ssh-ed25519', fingerprint: 'SHA256:abc' };

interface Listeners {
  onEvent: (event: SessionEvent) => void;
  onOutput: (data: Uint8Array) => void;
}

/** Ouvre un onglet et une session ; renvoie de quoi simuler le cœur. */
async function connected(sessionId = 7) {
  const tabId = useSessions.getState().openTab();
  const session = new TerminalSession(tabId);
  let listeners: Listeners | undefined;
  vi.mocked(ipc.sshConnect).mockImplementationOnce((_request, l) => {
    listeners = l;
    return Promise.resolve(sessionId);
  });
  await session.connect(TARGET, { password: 'secret' });
  const emit = (event: SessionEvent) => {
    listeners?.onEvent(event);
  };
  const output = (data: Uint8Array) => {
    listeners?.onOutput(data);
  };
  const tab = () => useSessions.getState().tabs.find((t) => t.id === tabId);
  return { session, tabId, emit, output, tab };
}

// jsdom n'implémente pas ResizeObserver.
vi.stubGlobal(
  'ResizeObserver',
  class {
    observe = vi.fn();
    disconnect = vi.fn();
  },
);

describe('TerminalSession', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSessions.setState({ tabs: [], activeTabId: HOME_TAB_ID });
  });

  it('envoie le secret une seule fois, dans la demande de connexion, sans le stocker', async () => {
    const { tab } = await connected();
    expect(ipc.sshConnect).toHaveBeenCalledTimes(1);
    expect(vi.mocked(ipc.sshConnect).mock.calls[0]?.[0]).toEqual({
      host: 'srv.lab',
      port: 22,
      username: 'root',
      auth: { kind: 'password', password: 'secret' },
      cols: 80,
      rows: 24,
    });
    expect(JSON.stringify(useSessions.getState())).not.toContain('secret');
    expect(tab()).toMatchObject({ status: 'connecting', target: TARGET });
  });

  it('suit la clé d’hôte inconnue, la décision et la connexion', async () => {
    const { session, emit, tab } = await connected();
    emit({ type: 'hostKeyUnknown', key: KEY });
    expect(tab()).toMatchObject({ status: 'hostKey', pendingHostKey: KEY });

    await session.decideHostKey(true);
    expect(ipc.sshHostKeyDecision).toHaveBeenCalledWith(7, true);
    emit({ type: 'authenticating' });
    emit({ type: 'connected' });
    expect(tab()).toMatchObject({ status: 'connected', pendingHostKey: null, errorCode: null });
  });

  it('suit les questions du serveur (2FA) sans conserver les réponses', async () => {
    const { session, emit, tab } = await connected();
    emit({ type: 'authenticating' });
    const prompt = {
      round: 1,
      name: '',
      instructions: '',
      prompts: [{ text: 'Verification code: ', echo: false }],
    };
    emit({ type: 'authPrompt', ...prompt });
    expect(tab()).toMatchObject({ status: 'authPrompt', pendingPrompt: prompt });

    await session.answerPrompt(['123456']);
    expect(ipc.sshAuthAnswer).toHaveBeenCalledWith(7, 1, ['123456']);
    expect(tab()).toMatchObject({ status: 'authenticating', pendingPrompt: null });
    expect(JSON.stringify(useSessions.getState())).not.toContain('123456');
    emit({ type: 'authenticating' });
    emit({ type: 'connected' });
    expect(tab()?.status).toBe('connected');
  });

  it('transmet l’annulation des questions et revient au formulaire', async () => {
    const { session, emit, tab } = await connected();
    emit({
      type: 'authPrompt',
      round: 4,
      name: '',
      instructions: '',
      prompts: [{ text: 'Code', echo: true }],
    });
    await session.answerPrompt(null);
    expect(ipc.sshAuthAnswer).toHaveBeenCalledWith(7, 4, null);
    emit({ type: 'disconnected', code: 'ssh.auth_cancelled' });
    expect(tab()).toMatchObject({
      status: 'form',
      errorCode: 'ssh.auth_cancelled',
      pendingPrompt: null,
    });
  });

  it('réaffiche la question si le cœur refuse les réponses', async () => {
    const { session, emit, tab } = await connected();
    const prompt = {
      round: 2,
      name: '',
      instructions: '',
      prompts: [{ text: 'Code', echo: false }],
    };
    emit({ type: 'authPrompt', ...prompt });
    vi.mocked(ipc.sshAuthAnswer).mockRejectedValueOnce({ code: 'ssh.invalid_answers' });
    await session.answerPrompt(['x'.repeat(2000)]);
    expect(tab()).toMatchObject({
      status: 'authPrompt',
      pendingPrompt: prompt,
      errorCode: 'ssh.invalid_answers',
    });
  });

  it('se connecte en mode interactif sans aucun secret', async () => {
    const tabId = useSessions.getState().openTab();
    const session = new TerminalSession(tabId);
    vi.mocked(ipc.sshConnect).mockResolvedValueOnce(3);
    await session.connect({ ...TARGET, authKind: 'interactive' }, {});
    expect(vi.mocked(ipc.sshConnect).mock.calls[0]?.[0]).toMatchObject({
      auth: { kind: 'interactive' },
    });
  });

  it('revient au formulaire avec la cause si la connexion échoue avant le shell', async () => {
    const { emit, tab } = await connected();
    emit({ type: 'authenticating' });
    emit({ type: 'disconnected', code: 'ssh.auth_failed' });
    expect(tab()).toMatchObject({ status: 'form', errorCode: 'ssh.auth_failed' });
  });

  it('passe en « déconnecté » après une session établie', async () => {
    const { emit, tab } = await connected();
    emit({ type: 'connected' });
    emit({ type: 'disconnected', code: 'ssh.connection_lost' });
    expect(tab()).toMatchObject({ status: 'disconnected', errorCode: 'ssh.connection_lost' });
  });

  it('garde l’alerte de clé modifiée affichée après la fin de session', async () => {
    const { emit, tab } = await connected();
    emit({
      type: 'hostKeyChanged',
      key: KEY,
      known: [KEY],
      hostPattern: 'srv.lab',
    });
    emit({ type: 'disconnected', code: 'ssh.host_key_changed' });
    expect(tab()).toMatchObject({
      status: 'blocked',
      hostKeyChange: { key: KEY, known: [KEY], hostPattern: 'srv.lab' },
    });
  });

  it('acquitte la sortie une fois affichée (contrôle de flux)', async () => {
    const { session, output } = await connected();
    output(new Uint8Array([104, 105]));
    expect(fake(session).written).toHaveLength(1);
    expect(ipc.sshAck).toHaveBeenCalledWith(7, 2);
  });

  it('affiche une erreur de validation renvoyée par le cœur', async () => {
    const tabId = useSessions.getState().openTab();
    const session = new TerminalSession(tabId);
    vi.mocked(ipc.sshConnect).mockRejectedValueOnce({ code: 'ssh.key_wrong_passphrase' });
    await session.connect(TARGET, { password: 'x' });
    expect(useSessions.getState().tabs[0]).toMatchObject({
      status: 'form',
      errorCode: 'ssh.key_wrong_passphrase',
    });
  });

  it('ignore les messages de l’ancienne session après une reconnexion', async () => {
    const first = await connected(1);
    first.emit({ type: 'connected' });
    vi.mocked(ipc.sshConnect).mockResolvedValueOnce(2);
    await first.session.connect(TARGET, { password: 'secret' });
    expect(ipc.sshClose).toHaveBeenCalledWith(1);
    first.emit({ type: 'disconnected', code: 'ssh.connection_lost' });
    expect(first.tab()?.status).toBe('connecting');
  });

  it('fait confirmer un collage multi-lignes et colle directement une ligne simple', async () => {
    const { session } = await connected();
    const confirmPaste = vi.fn();
    session.attach(document.createElement('div'), { confirmPaste, confirmLink: vi.fn() });

    session.paste('ls -la');
    expect(fake(session).paste).toHaveBeenCalledWith('ls -la');

    session.paste('rm -rf /tmp/x\nreboot\n');
    expect(confirmPaste).toHaveBeenCalledWith(
      expect.objectContaining({ lineCount: 3, needsConfirmation: true }),
    );
    expect(fake(session).paste).toHaveBeenCalledTimes(1);
  });

  it('ferme la session distante à la fermeture de l’onglet', async () => {
    const { session } = await connected(9);
    session.dispose();
    expect(ipc.sshClose).toHaveBeenCalledWith(9);
    expect(fake(session).dispose).toHaveBeenCalled();
  });

  it('se connecte à un hôte du carnet par son identifiant, sans aucun secret', async () => {
    const tabId = useSessions.getState().openTab();
    const session = new TerminalSession(tabId);
    vi.mocked(ipc.sshConnectHost).mockResolvedValueOnce(9);
    await session.connectHost({ ...TARGET, hostId: 'h-1', label: 'Web' });
    expect(ipc.sshConnectHost).toHaveBeenCalledWith(
      { hostId: 'h-1', cols: 80, rows: 24 },
      expect.anything(),
    );
    expect(JSON.stringify(vi.mocked(ipc.sshConnectHost).mock.calls[0]?.[0])).not.toContain(
      'password',
    );
    const tab = useSessions.getState().tabs.find((t) => t.id === tabId);
    expect(tab?.target).toMatchObject({ hostId: 'h-1', label: 'Web' });
    session.dispose();
  });

  it('applique à chaud la taille de police des paramètres', () => {
    const session = new TerminalSession('police');
    useSettings.setState({ terminalFontSize: 18 });
    expect(session.terminal.options.fontSize).toBe(18);
    session.dispose();
    // Après destruction, le terminal ne suit plus les paramètres.
    useSettings.setState({ terminalFontSize: 20 });
    expect(session.terminal.options.fontSize).toBe(18);
    useSettings.setState({ terminalFontSize: 14 });
  });

  it('ferme tous les onglets et détruit leurs terminaux au verrouillage', async () => {
    const first = useSessions.getState().openTab();
    const second = useSessions.getState().openTab();
    const a = terminalSession(first);
    const b = terminalSession(second);
    closeAllTabs();
    expect(useSessions.getState().tabs).toHaveLength(0);
    expect(useSessions.getState().activeTabId).toBe(HOME_TAB_ID);
    expect(fake(a).dispose).toHaveBeenCalled();
    expect(fake(b).dispose).toHaveBeenCalled();
    // Le registre est vidé : un nouvel accès crée un nouveau terminal.
    expect(terminalSession(first)).not.toBe(a);
    closeAllTabs();
    await Promise.resolve();
  });
});
