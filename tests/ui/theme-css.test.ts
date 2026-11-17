// @vitest-environment node
//
// Régression : la palette sombre était déclarée dans `@layer base` alors que la palette claire
// (`:root`) ne l'était pas. Une règle hors couche l'emporte toujours sur une règle en couche :
// la classe `dark` était bien posée, mais les couleurs claires restaient affichées.
// jsdom ne calcule pas ces styles, d'où ce contrôle de structure de la feuille de style.
import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

// Les commentaires sont retirés : ils se colleraient sinon au nom des blocs analysés.
const css = readFileSync(new URL('../../src/index.css', import.meta.url), 'utf8').replace(
  /\/\*[\s\S]*?\*\//g,
  '',
);

/** Liste des blocs `@layer` ouverts à l'endroit donné de la feuille de style. */
function enclosingAtRules(index: number): string[] {
  const stack: string[] = [];
  let header = '';
  for (const char of css.slice(0, index)) {
    if (char === '{') {
      stack.push(header.trim());
      header = '';
    } else if (char === '}') {
      stack.pop();
      header = '';
    } else if (char === ';') {
      header = '';
    } else {
      header += char;
    }
  }
  return stack;
}

describe('palettes de thème (src/index.css)', () => {
  it.each([':root {', '.dark {', ':root:not(.light) {'])(
    'déclare « %s » hors de toute couche @layer',
    (selector) => {
      const index = css.indexOf(selector);
      expect(index, `${selector} introuvable`).toBeGreaterThanOrEqual(0);
      const layers = enclosingAtRules(index).filter((rule) => rule.startsWith('@layer'));
      expect(layers).toEqual([]);
    },
  );

  it('redéfinit les couleurs principales en thème sombre', () => {
    const dark = css.slice(css.indexOf('.dark {'), css.indexOf('}', css.indexOf('.dark {')));
    for (const variable of ['--background', '--foreground', '--sidebar', '--border']) {
      expect(dark).toContain(`${variable}:`);
    }
  });
});
