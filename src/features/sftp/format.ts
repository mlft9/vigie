// Mise en forme et chemins pour l'explorateur SFTP.

import type { EntryKind } from '@/lib/types';

const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB'] as const;

/** Taille lisible (unités binaires, 1 décimale au-delà de l'octet). */
export function formatSize(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${String(value)} B` : `${value.toFixed(1)} ${UNITS[unit] ?? ''}`;
}

/** Droits Unix au format `rwxr-xr-x` (bits spéciaux compris : s, S, t, T). */
export function formatPermissions(mode: number | null): string {
  if (mode === null) return '';
  const triplet = (shift: number, special: boolean, specialChar: string) => {
    const r = mode & (4 << shift) ? 'r' : '-';
    const w = mode & (2 << shift) ? 'w' : '-';
    const x = (mode & (1 << shift)) !== 0;
    let exec = x ? 'x' : '-';
    if (special) exec = x ? specialChar : specialChar.toUpperCase();
    return r + w + exec;
  };
  return (
    triplet(6, (mode & 0o4000) !== 0, 's') +
    triplet(3, (mode & 0o2000) !== 0, 's') +
    triplet(0, (mode & 0o1000) !== 0, 't')
  );
}

/** Date de modification lisible dans la langue courante. */
export function formatDate(seconds: number | null, locale: string): string {
  if (seconds === null) return '';
  return new Date(seconds * 1000).toLocaleString(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
}

/** Chemin distant d'un élément d'un dossier. */
export function joinRemote(dir: string, name: string): string {
  return dir === '/' ? `/${name}` : `${dir}/${name}`;
}

/** Dossier parent d'un chemin distant absolu. */
export function parentRemote(path: string): string {
  const index = path.lastIndexOf('/');
  return index <= 0 ? '/' : path.slice(0, index);
}

/** Chemin relatif d'un élément d'un dossier local (`""` = dossier choisi). */
export function joinLocal(dir: string, name: string): string {
  return dir === '' ? name : `${dir}/${name}`;
}

/** Dossier parent d'un chemin local relatif. */
export function parentLocal(path: string): string {
  const index = path.lastIndexOf('/');
  return index === -1 ? '' : path.slice(0, index);
}

/** Vrai si l'élément s'ouvre comme un dossier (dossier, ou lien vers un dossier). */
export function isDirLike(kind: EntryKind, linkTarget: EntryKind | null = null): boolean {
  return kind === 'dir' || (kind === 'symlink' && linkTarget === 'dir');
}

/** Valeur octale saisie (3 ou 4 chiffres de 0 à 7), sinon `null`. */
export function parseOctal(text: string): number | null {
  return /^[0-7]{3,4}$/.test(text) ? parseInt(text, 8) : null;
}
