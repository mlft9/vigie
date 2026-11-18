import { useTranslation } from 'react-i18next';

import type { HostKeyInfo } from '@/lib/types';

/** Algorithme et empreinte d'une clé d'hôte, en police à chasse fixe et sélectionnables. */
export function Fingerprint({ label, hostKey }: { label: string; hostKey: HostKeyInfo }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-1 rounded-md border bg-muted/50 px-3 py-2">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <span className="font-mono text-xs">{hostKey.algorithm}</span>
      <span className="font-mono text-sm break-all select-all">
        {/* Empreinte vide : ligne enregistrée illisible, qui bloque quand même la connexion. */}
        {hostKey.fingerprint || t('hostKey.unreadable')}
      </span>
    </div>
  );
}
