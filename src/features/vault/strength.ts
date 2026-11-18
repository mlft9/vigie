// Estimation de la robustesse du mot de passe maître, **en direct**, dans l'interface.
//
// Elle sert uniquement d'indicateur (VLT-01) : le cœur refait le contrôle avec zxcvbn (Rust)
// à la création et fait foi (SECURITE.md §3.4). L'estimation est faite ici pour ne pas envoyer
// le mot de passe au cœur à chaque frappe (un secret ne traverse l'IPC qu'une fois).
//
// Les dictionnaires (plusieurs Mo) sont chargés à la demande, dans un fichier séparé.

import type { ZxcvbnFactory } from '@zxcvbn-ts/core';

/** Longueur minimale exigée par le cœur. */
export const MIN_MASTER_LENGTH = 12;
/** Score zxcvbn minimal exigé par le cœur. */
export const MIN_SCORE = 3;

export type StrengthScore = 0 | 1 | 2 | 3 | 4;

let factory: Promise<ZxcvbnFactory> | null = null;

function loadFactory(): Promise<ZxcvbnFactory> {
  factory ??= Promise.all([
    import('@zxcvbn-ts/core'),
    import('@zxcvbn-ts/language-common'),
    import('@zxcvbn-ts/language-en'),
  ]).then(
    ([core, common, en]) =>
      new core.ZxcvbnFactory({
        dictionary: { ...common.dictionary, ...en.dictionary },
        graphs: common.adjacencyGraphs,
      }),
  );
  return factory;
}

/** Nombre de caractères (et non d'unités UTF-16), comme le cœur. */
export function characterCount(password: string): number {
  return Array.from(password).length;
}

/** Score de 0 (très faible) à 4 (très robuste). */
export async function estimateStrength(password: string): Promise<StrengthScore> {
  if (password === '') return 0;
  const zxcvbn = await loadFactory();
  return zxcvbn.check(password).score;
}

/** Vrai si le mot de passe devrait être accepté par le cœur. */
export function meetsPolicy(password: string, score: StrengthScore): boolean {
  return characterCount(password) >= MIN_MASTER_LENGTH && score >= MIN_SCORE;
}
