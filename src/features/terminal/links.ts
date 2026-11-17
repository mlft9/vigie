// Politique d'ouverture des liens du terminal (SECURITE.md §4.2, décision du porteur).
//
// - Seuls http: et https: sont ouverts, jamais file:, javascript:, ni un schéma d'application.
// - L'ouverture demande un geste explicite : Ctrl+clic (Cmd+clic n'existe pas sur nos
//   plateformes), pour qu'un simple clic de sélection n'ouvre rien.
// - Un lien OSC 8 peut afficher un texte différent de sa cible : il passe par une
//   confirmation qui montre l'adresse réelle.
// Le cœur Rust revalide toujours l'adresse avant d'ouvrir le navigateur.

/** Longueur maximale d'une adresse ouverte (identique au cœur). */
export const MAX_URL_LENGTH = 2048;

/** Vrai si l'adresse peut être proposée à l'ouverture. */
export function isOpenableUrl(raw: string): boolean {
  if (raw.length > MAX_URL_LENGTH) return false;
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f\x7f]/.test(raw)) return false;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  return (
    (url.protocol === 'http:' || url.protocol === 'https:') &&
    url.hostname !== '' &&
    url.username === '' &&
    url.password === ''
  );
}

/** Vrai si le clic est un geste d'ouverture explicite. */
export function isOpenGesture(event: Pick<MouseEvent, 'ctrlKey' | 'metaKey'>): boolean {
  return event.ctrlKey || event.metaKey;
}
