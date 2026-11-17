import { detectLanguage } from './languages';

describe('detectLanguage', () => {
  it.each([
    [['fr-FR'], 'fr'],
    [['fr-CA', 'en-US'], 'fr'],
    [['FR'], 'fr'],
    [['en-GB'], 'en'],
    [['de-DE', 'fr-FR'], 'fr'],
    [['de-DE', 'es'], 'en'],
    [[], 'en'],
    [[''], 'en'],
  ] as const)('%j → %s', (preferred, expected) => {
    expect(detectLanguage(preferred)).toBe(expected);
  });
});
