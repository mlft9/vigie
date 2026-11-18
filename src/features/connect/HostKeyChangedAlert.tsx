import { ShieldAlert } from 'lucide-react';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import type { HostKeyChange } from '@/stores/sessions';

import { Fingerprint } from './Fingerprint';

interface HostKeyChangedAlertProps {
  host: string;
  port: number;
  change: HostKeyChange;
  onClose: () => void;
  /** Ouvre la gestion des clés d'hôte connues (HKY-04). */
  onManage: () => void;
}

/**
 * Clé d'hôte modifiée (HKY-02) : la connexion est refusée et il n'existe **aucun** bouton
 * pour continuer. Le déblocage passe par la suppression volontaire de l'ancienne clé, après
 * vérification, dans la gestion des clés d'hôte connues (HKY-04).
 */
export function HostKeyChangedAlert({
  host,
  port,
  change,
  onClose,
  onManage,
}: HostKeyChangedAlertProps) {
  const { t } = useTranslation();
  const id = useId();
  const address = port === 22 ? host : `${host}:${port}`;

  return (
    <section
      role="alert"
      aria-labelledby={`${id}-title`}
      className="flex w-full max-w-xl flex-col gap-4 rounded-lg border-2 border-destructive bg-background p-6 shadow-lg"
    >
      <h2
        id={`${id}-title`}
        className="flex items-center gap-2 text-lg font-semibold text-destructive"
      >
        <ShieldAlert aria-hidden="true" className="size-6" />
        {t('hostKey.changed.title')}
      </h2>
      <p className="text-sm font-medium">{t('hostKey.changed.warning', { host: address })}</p>
      <p className="text-sm">{t('hostKey.changed.explanation')}</p>
      <Fingerprint label={t('hostKey.presented')} hostKey={change.key} />
      {change.known.map((known, index) => (
        // Position plutôt qu'empreinte : une ligne illisible a une empreinte vide.
        <Fingerprint key={index} label={t('hostKey.changed.known')} hostKey={known} />
      ))}
      <div className="flex flex-col gap-1 text-sm">
        <h3 className="font-medium">{t('hostKey.changed.unblockTitle')}</h3>
        <p className="text-muted-foreground">
          {t('hostKey.changed.unblock', { host: change.hostPattern })}
        </p>
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={onManage}>
          {t('hostKey.changed.manage')}
        </Button>
        <Button variant="outline" onClick={onClose}>
          {t('hostKey.changed.close')}
        </Button>
      </div>
    </section>
  );
}
