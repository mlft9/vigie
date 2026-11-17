import { isAppError, toAppError, UNKNOWN_ERROR_CODE } from './ipc';

describe('normalisation des erreurs IPC', () => {
  it('conserve un code valide venant du backend', () => {
    expect(toAppError({ code: 'ipc.command_denied' })).toEqual({ code: 'ipc.command_denied' });
  });

  it('ne propage jamais les champs supplémentaires', () => {
    expect(toAppError({ code: 'ssh.auth_failed', detail: 'mot de passe: hunter2' })).toEqual({
      code: 'ssh.auth_failed',
    });
  });

  it.each([
    'chaîne brute',
    new Error('boom'),
    null,
    { code: 42 },
    { code: 'Majuscules.Interdites' },
    { code: '../../etc' },
    { code: 'a..b' },
  ])('remplace une erreur inattendue (%j) par le code générique', (value) => {
    expect(isAppError(value)).toBe(false);
    expect(toAppError(value)).toEqual({ code: UNKNOWN_ERROR_CODE });
  });
});
