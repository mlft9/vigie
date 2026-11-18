// @vitest-environment node
import { describe, expect, it } from 'vitest';

import { loadIsolation } from './load-isolation';

describe("hook d'isolation", () => {
  const isolation = loadIsolation();
  const hook = isolation.__TAURI_ISOLATION_HOOK__;
  const { deniedCommand } = isolation.__YOUNJI_ISOLATION__;

  it('laisse passer une commande autorisée sans la modifier', () => {
    const message = { cmd: 'app_info', callback: 1, error: 2, payload: {}, options: {} };
    expect(hook(message)).toBe(message);
  });

  it('laisse passer la récupération des messages de Channel (interne à Tauri)', () => {
    const message = { cmd: 'plugin:__TAURI_CHANNEL__|fetch', callback: 1, error: 2, payload: null };
    expect(hook(message)).toBe(message);
  });

  it('limite les commandes internes à la seule récupération des Channel', () => {
    expect(isolation.__YOUNJI_ISOLATION__.internalCommands).toEqual([
      'plugin:__TAURI_CHANNEL__|fetch',
    ]);
  });

  it.each([
    'plugin:__TAURI_CHANNEL__|other',
    'plugin:__TAURI_CHANNEL__|fetch ',
    'plugin:shell|execute',
    'plugin:fs|read_file',
    'APP_INFO',
    'app_info ',
    'constructor',
    '__proto__',
    '',
  ])('refuse la commande %j et ne transmet pas sa charge utile', (cmd) => {
    const result = hook({ cmd, callback: 7, error: 8, payload: { secret: 'x' } });
    expect(result.cmd).toBe(deniedCommand);
    expect(result.callback).toBe(7);
    expect(result.error).toBe(8);
    expect(Object.keys(result.payload as object)).toEqual([]);
  });

  it.each([null, undefined, 42, 'app_info', { cmd: 42 }, { cmd: ['app_info'] }])(
    'refuse un message malformé (%j)',
    (message) => {
      expect(hook(message).cmd).toBe(deniedCommand);
    },
  );

  it('ne permet pas de modifier la liste blanche', () => {
    const list = isolation.__YOUNJI_ISOLATION__.allowedCommands as string[];
    expect(() => {
      list.push('plugin:shell|execute');
    }).toThrow();
    expect(hook({ cmd: 'plugin:shell|execute', callback: 1, error: 2, payload: {} }).cmd).toBe(
      deniedCommand,
    );
  });
});

describe("protection du hook d'isolation", () => {
  it('ne peut être ni remplacé ni supprimé', () => {
    const isolation = loadIsolation();
    const original = isolation.__TAURI_ISOLATION_HOOK__;
    expect(Reflect.set(isolation, '__TAURI_ISOLATION_HOOK__', () => null)).toBe(false);
    expect(Reflect.deleteProperty(isolation, '__TAURI_ISOLATION_HOOK__')).toBe(false);
    expect(isolation.__TAURI_ISOLATION_HOOK__).toBe(original);
  });
});
