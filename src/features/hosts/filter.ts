import type { EntryId, HostEntry } from '@/lib/types';

/** Découpe la saisie « a, b, c » en étiquettes. */
export function parseTags(text: string): string[] {
  return text
    .split(',')
    .map((tag) => tag.trim())
    .filter((tag) => tag !== '');
}

/** Utilisateur affiché : celui de l'hôte, sinon celui de l'identité. */
export function effectiveUsername(hostUsername: string, identityUsername?: string): string {
  return hostUsername !== '' ? hostUsername : (identityUsername ?? '');
}

/** Filtre de groupe : tous les hôtes, ceux sans groupe, ou un groupe donné. */
export type GroupFilter = { kind: 'all' } | { kind: 'ungrouped' } | { kind: 'group'; id: EntryId };

/** Texte normalisé pour la recherche : minuscules, sans accents. */
function normalize(text: string): string {
  return text
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '')
    .toLowerCase();
}

/**
 * Hôtes du groupe choisi dont le nom, l'adresse, l'utilisateur ou une étiquette contient
 * chacun des mots recherchés (HST-02, recherche instantanée). Tri par nom.
 */
export function filterHosts(
  hosts: readonly HostEntry[],
  group: GroupFilter,
  query: string,
): HostEntry[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  return hosts
    .filter((host) => {
      if (group.kind === 'ungrouped' && host.groupId !== null) return false;
      if (group.kind === 'group' && host.groupId !== group.id) return false;
      const haystack = normalize(
        [host.name, host.address, host.username, ...host.tags, String(host.port)].join(' '),
      );
      return words.every((word) => haystack.includes(word));
    })
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
}
