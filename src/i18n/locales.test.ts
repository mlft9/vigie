import en from '@/locales/en.json';
import fr from '@/locales/fr.json';

/** Liste toutes les clés « feuilles » d'un objet de traductions (a.b.c). */
function leafKeys(node: unknown, prefix = ''): string[] {
  if (typeof node !== 'object' || node === null) {
    return [prefix];
  }
  return Object.entries(node).flatMap(([key, value]) =>
    leafKeys(value, prefix ? `${prefix}.${key}` : key),
  );
}

function leafValues(node: unknown): unknown[] {
  if (typeof node !== 'object' || node === null) {
    return [node];
  }
  return Object.values(node).flatMap(leafValues);
}

describe('fichiers de traduction', () => {
  it('ont exactement les mêmes clés en français et en anglais', () => {
    expect(leafKeys(en).sort()).toEqual(leafKeys(fr).sort());
  });

  it('ne contiennent que des chaînes non vides', () => {
    for (const value of [...leafValues(fr), ...leafValues(en)]) {
      expect(typeof value).toBe('string');
      expect((value as string).trim()).not.toBe('');
    }
  });

  it('utilisent les mêmes variables d’interpolation dans les deux langues', () => {
    const vars = (s: string) => [...s.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();
    const get = (root: unknown, key: string): string =>
      key
        .split('.')
        .reduce<unknown>((acc, k) => (acc as Record<string, unknown>)[k], root) as string;
    for (const key of leafKeys(fr)) {
      expect(vars(get(en, key)), key).toEqual(vars(get(fr, key)));
    }
  });
});
