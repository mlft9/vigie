// Non-régression (étape 1) : l'application doit se charger avec Object.prototype gelé.
//
// Tauri gèle Object.prototype avant tout script (`freezePrototype: true`, durcissement contre
// la pollution de prototype). xterm.js 6 écrivait `toString` sur un objet ordinaire dès son
// chargement : l'exception empêchait tout le JavaScript de s'exécuter et la fenêtre restait
// blanche, sans qu'aucun test ne le voie. Ce test reproduit le gel puis charge et affiche
// l'application et un terminal.
//
// Chaque fichier de test tourne dans son propre processus (Vitest, `isolate` par défaut) :
// le gel ne déborde pas sur les autres tests.
import { render, screen } from '@testing-library/react';

// Gel avant tout import dynamique, comme le script injecté en premier par Tauri.
Object.freeze(Object.prototype);

// jsdom n'implémente ni matchMedia (utilisé par xterm.js) ni ResizeObserver.
vi.stubGlobal('matchMedia', (query: string) => ({
  matches: false,
  media: query,
  addListener: vi.fn(),
  removeListener: vi.fn(),
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
}));
vi.stubGlobal(
  'ResizeObserver',
  class {
    observe = vi.fn();
    disconnect = vi.fn();
  },
);

describe('chargement avec Object.prototype gelé (freezePrototype de Tauri)', () => {
  it('le prototype est bien gelé pendant ce test', () => {
    expect(Object.isFrozen(Object.prototype)).toBe(true);
  });

  it('charge les modules de xterm.js', async () => {
    await expect(import('@xterm/xterm')).resolves.toBeDefined();
    for (const load of [
      () => import('@xterm/addon-fit'),
      () => import('@xterm/addon-search'),
      () => import('@xterm/addon-web-links'),
      () => import('@xterm/addon-unicode11'),
      () => import('@xterm/addon-webgl'),
    ]) {
      await expect(load()).resolves.toBeDefined();
    }
  });

  it("affiche l'application", async () => {
    const { App } = await import('./App');
    render(<App />);
    // La langue suit le système : on cherche la barre d'onglets, pas un libellé.
    expect(await screen.findByRole('tablist')).toBeInTheDocument();
    expect(screen.getByRole('navigation')).toBeInTheDocument();
  });

  it('crée et attache un terminal', async () => {
    const { TerminalSession } = await import('./features/terminal/TerminalSession');
    const session = new TerminalSession('frozen-test');
    expect(() => {
      session.attach(document.createElement('div'), {
        confirmPaste: vi.fn(),
        confirmLink: vi.fn(),
      });
    }).not.toThrow();
    session.dispose();
  });
});
