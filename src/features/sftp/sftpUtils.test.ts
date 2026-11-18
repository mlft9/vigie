import { useSftp } from '@/stores/sftp';

import { readLocalDrag, readRemoteDrag, REMOTE_DRAG_TYPE, LOCAL_DRAG_TYPE } from './dnd';
import {
  formatPermissions,
  formatSize,
  isDirLike,
  joinLocal,
  joinRemote,
  parentLocal,
  parentRemote,
  parseOctal,
} from './format';
import { parseOsc7, SHELL_INTEGRATION } from './osc7';

describe('OSC 7', () => {
  it('décode le chemin et ignore le nom d’hôte', () => {
    expect(parseOsc7('file://srv/home/alice')).toBe('/home/alice');
    expect(parseOsc7('file:///var/log')).toBe('/var/log');
    expect(parseOsc7('file://h/a%20b/%C3%A9t%C3%A9%25x')).toBe('/a b/été%x');
    // Chemin brut non encodé (shells qui n'encodent pas) : accepté s'il reste décodable.
    expect(parseOsc7('file://h/a b/été')).toBe('/a b/été');
  });

  it('refuse les formes invalides et les caractères de contrôle', () => {
    for (const bad of [
      '',
      '/home/alice',
      'http://h/x',
      'file://hôte-sans-chemin',
      'file://h/%E9',
      'file://h/a%00b',
      'file://h/a%0Ab',
      'file://h/a%1B[31m',
      'file://h/a\u0085b',
      `file://h/${'a'.repeat(5000)}`,
    ]) {
      expect(parseOsc7(bad)).toBeNull();
    }
  });

  it('envoie une intégration au prompt constante, hors historique, sur une seule ligne', () => {
    expect(SHELL_INTEGRATION.startsWith(' ')).toBe(true);
    expect(SHELL_INTEGRATION.endsWith('\r')).toBe(true);
    expect(SHELL_INTEGRATION.slice(0, -1)).not.toMatch(/[\r\n]/);
    // Pas de « ! » (expansion de l'historique de bash).
    expect(SHELL_INTEGRATION).not.toContain('!');
    expect(SHELL_INTEGRATION).toContain("printf '\\033]7;file://%s%s\\033\\\\'");
  });
});

describe('mise en forme', () => {
  it('formate les droits, bits spéciaux compris', () => {
    expect(formatPermissions(0o755)).toBe('rwxr-xr-x');
    expect(formatPermissions(0o640)).toBe('rw-r-----');
    expect(formatPermissions(0o4755)).toBe('rwsr-xr-x');
    expect(formatPermissions(0o1777)).toBe('rwxrwxrwt');
    expect(formatPermissions(0o2644)).toBe('rw-r-Sr--');
    expect(formatPermissions(null)).toBe('');
  });

  it('formate les tailles et lit une valeur octale', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(1536)).toBe('1.5 KiB');
    expect(formatSize(1024 ** 3)).toBe('1.0 GiB');
    expect(parseOctal('644')).toBe(0o644);
    expect(parseOctal('4755')).toBe(0o4755);
    for (const bad of ['', '8', '64', '77777', 'abc', '0o644']) expect(parseOctal(bad)).toBeNull();
  });

  it('construit les chemins distants et locaux', () => {
    expect(joinRemote('/', 'etc')).toBe('/etc');
    expect(joinRemote('/home/a', 'b c')).toBe('/home/a/b c');
    expect(parentRemote('/home/a')).toBe('/home');
    expect(parentRemote('/home')).toBe('/');
    expect(parentRemote('/')).toBe('/');
    expect(joinLocal('', 'x')).toBe('x');
    expect(joinLocal('a/b', 'x')).toBe('a/b/x');
    expect(parentLocal('a/b')).toBe('a');
    expect(parentLocal('a')).toBe('');
    expect(isDirLike('symlink', 'dir')).toBe(true);
    expect(isDirLike('symlink', null)).toBe(false);
  });
});

/** Double minimal de DataTransfer (jsdom ne l'implémente pas). */
function transfer(data: Record<string, string>): DataTransfer {
  return { getData: (type: string) => data[type] ?? '' } as unknown as DataTransfer;
}

describe('glisser-déposer', () => {
  it('valide les données déposées', () => {
    expect(
      readRemoteDrag(
        transfer({ [REMOTE_DRAG_TYPE]: JSON.stringify({ sftpId: 3, paths: ['/a'] }) }),
      ),
    ).toEqual({ sftpId: 3, paths: ['/a'] });
    expect(
      readLocalDrag(transfer({ [LOCAL_DRAG_TYPE]: JSON.stringify({ rootId: 2, paths: ['x/y'] }) })),
    ).toEqual({ rootId: 2, paths: ['x/y'] });
    for (const raw of [
      '',
      'pas du json',
      'null',
      JSON.stringify({ sftpId: '3', paths: ['/a'] }),
      JSON.stringify({ sftpId: 3, paths: [] }),
      JSON.stringify({ sftpId: 3, paths: [1] }),
      JSON.stringify({ sftpId: 3, paths: Array.from({ length: 1001 }, () => '/a') }),
    ]) {
      expect(readRemoteDrag(transfer({ [REMOTE_DRAG_TYPE]: raw }))).toBeNull();
    }
    // Un dépôt d'un autre type (fichier du système, texte) est ignoré.
    expect(readLocalDrag(transfer({ 'text/plain': '/etc/passwd' }))).toBeNull();
  });
});

describe('store des transferts', () => {
  beforeEach(() => {
    useSftp.getState().reset();
  });

  it('suit un transfert de la file à la fin', () => {
    const { handleEvent } = useSftp.getState();
    handleEvent(1, { type: 'transferQueued', job: 5, direction: 'download', label: 'a', count: 2 });
    handleEvent(1, { type: 'transferStarted', job: 5 });
    handleEvent(1, {
      type: 'transferProgress',
      job: 5,
      bytesDone: 10,
      bytesTotal: 100,
      filesDone: 0,
      filesTotal: 2,
    });
    expect(useSftp.getState().transfers[0]).toMatchObject({ status: 'running', bytesDone: 10 });
    handleEvent(1, { type: 'transferConflict', job: 5, name: 'a' });
    expect(useSftp.getState().transfers[0]).toMatchObject({
      status: 'conflict',
      conflictName: 'a',
    });
    handleEvent(1, {
      type: 'transferFinished',
      job: 5,
      transferred: 1,
      skipped: 0,
      ignored: 0,
      refused: 1,
      failed: 0,
      code: null,
    });
    expect(useSftp.getState().transfers[0]?.status).toBe('done');
    expect(useSftp.getState().revision[1]).toBe(1);

    handleEvent(1, { type: 'transferQueued', job: 6, direction: 'upload', label: 'b', count: 1 });
    handleEvent(1, {
      type: 'transferFinished',
      job: 6,
      transferred: 0,
      skipped: 0,
      ignored: 0,
      refused: 0,
      failed: 0,
      code: 'ssh.transfer_cancelled',
    });
    expect(useSftp.getState().transfers[1]?.status).toBe('cancelled');
    useSftp.getState().clearFinished(1);
    expect(useSftp.getState().transfers).toEqual([]);
  });

  it('oublie tout ce qui concerne un client fermé', () => {
    const state = useSftp.getState();
    state.handleEvent(1, {
      type: 'transferQueued',
      job: 1,
      direction: 'upload',
      label: 'a',
      count: 1,
    });
    state.handleEvent(2, {
      type: 'transferQueued',
      job: 2,
      direction: 'upload',
      label: 'b',
      count: 1,
    });
    state.addEdit({ edit: 9, sftpId: 1, label: 'x' });
    state.handleEvent(1, { type: 'editConflict', edit: 9 });
    expect(useSftp.getState().edits[0]).toMatchObject({
      status: 'conflict',
      code: 'ssh.edit_conflict',
    });
    useSftp.getState().forget(1);
    expect(useSftp.getState().transfers.map((t) => t.job)).toEqual([2]);
    expect(useSftp.getState().edits).toEqual([]);
  });
});
