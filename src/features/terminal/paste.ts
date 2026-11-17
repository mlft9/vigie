// Garde de collage (SECURITE.md §4.2, protection contre le « pastejacking »).
//
// Un texte copié depuis une page web peut contenir des retours à la ligne ou des caractères
// de contrôle invisibles : collé dans un shell, il exécuterait des commandes à l'insu de
// l'utilisateur. Un tel collage demande donc une confirmation avec un aperçu où chaque
// caractère de contrôle est rendu visible.
//
// Les marqueurs du mode « bracketed paste » (ESC[200~ et ESC[201~) sont toujours retirés :
// un texte qui contiendrait ESC[201~ pourrait sinon sortir du mode collage et faire
// exécuter la suite comme si elle était tapée au clavier.

/** Marqueurs de début et de fin du mode « bracketed paste ». */
// eslint-disable-next-line no-control-regex
const BRACKETED_PASTE_MARKERS = /\x1b\[20[01]~/g;

/** Longueur maximale de l'aperçu affiché. */
export const PASTE_PREVIEW_MAX = 2000;

export interface PasteAnalysis {
  /** Texte à coller, sans les marqueurs de bracketed paste. */
  text: string;
  /** Vrai si le collage doit être confirmé par l'utilisateur. */
  needsConfirmation: boolean;
  /** Nombre de lignes du texte. */
  lineCount: number;
  /** Vrai si le texte contient des caractères de contrôle autres que tabulation et retour. */
  hasControlCharacters: boolean;
}

// Caractères de contrôle C0 (hors tabulation, saut de ligne et retour chariot), DEL et C1.
// eslint-disable-next-line no-control-regex
const CONTROL_CHARACTERS = /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f\x80-\x9f]/;

/** Analyse un texte avant de le coller dans le terminal. */
export function analyzePaste(raw: string): PasteAnalysis {
  const text = raw.replace(BRACKETED_PASTE_MARKERS, '');
  const lineCount = text.split(/\r\n|\r|\n/).length;
  const hasNewline = /[\r\n]/.test(text);
  const hasControlCharacters = CONTROL_CHARACTERS.test(text);
  return {
    text,
    needsConfirmation: hasNewline || hasControlCharacters,
    lineCount,
    hasControlCharacters,
  };
}

/**
 * Aperçu lisible : chaque caractère de contrôle est remplacé par son symbole Unicode
 * (␛ pour ESC, ␀ pour NUL…), les retours à la ligne restent visibles sous la forme ↵ suivie
 * d'un vrai saut de ligne. L'aperçu est tronqué à PASTE_PREVIEW_MAX caractères.
 */
export function pastePreview(text: string): { preview: string; truncated: boolean } {
  const truncated = text.length > PASTE_PREVIEW_MAX;
  const shown = truncated ? text.slice(0, PASTE_PREVIEW_MAX) : text;
  let preview = '';
  for (let i = 0; i < shown.length; i++) {
    const char = shown.charAt(i);
    const code = char.charCodeAt(0);
    if (char === '\r' && shown.charAt(i + 1) === '\n') {
      preview += '↵\n';
      i++;
    } else if (char === '\n' || char === '\r') {
      preview += '↵\n';
    } else if (char === '\t') {
      preview += '⇥';
    } else if (code < 0x20) {
      preview += String.fromCharCode(0x2400 + code);
    } else if (code === 0x7f) {
      preview += '␡';
    } else if (code >= 0x80 && code <= 0x9f) {
      preview += '�';
    } else {
      preview += char;
    }
  }
  return { preview, truncated };
}
