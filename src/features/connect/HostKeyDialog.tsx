import { ShieldQuestion } from 'lucide-react';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Modal } from '@/components/ui/modal';
import type { HostKeyInfo } from '@/lib/types';

import { Fingerprint } from './Fingerprint';

interface HostKeyDialogProps {
  host: string;
  port: number;
  hostKey: HostKeyInfo;
  onDecision: (accept: boolean) => void;
}

/**
 * Clé d'hôte inconnue (HKY-01) : type et empreinte SHA-256, acceptation explicite.
 * Le focus part sur « Refuser » : une touche Entrée machinale ne doit pas accepter une clé.
 */
export function HostKeyDialog({ host, port, hostKey, onDecision }: HostKeyDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const address = port === 22 ? host : `${host}:${port}`;

  return (
    <Modal
      role="alertdialog"
      labelledBy={`${id}-title`}
      describedBy={`${id}-description`}
      onClose={() => {
        onDecision(false);
      }}
    >
      <h2 id={`${id}-title`} className="flex items-center gap-2 text-lg font-semibold">
        <ShieldQuestion aria-hidden="true" className="size-5" />
        {t('hostKey.unknown.title')}
      </h2>
      <div id={`${id}-description`} className="flex flex-col gap-3 text-sm">
        <p>{t('hostKey.unknown.intro', { host: address })}</p>
        <Fingerprint label={t('hostKey.presented')} hostKey={hostKey} />
        <p className="text-muted-foreground">{t('hostKey.unknown.verify')}</p>
      </div>
      <div className="flex justify-end gap-2">
        <Button
          variant="outline"
          data-autofocus
          onClick={() => {
            onDecision(false);
          }}
        >
          {t('hostKey.unknown.reject')}
        </Button>
        <Button
          onClick={() => {
            onDecision(true);
          }}
        >
          {t('hostKey.unknown.accept')}
        </Button>
      </div>
    </Modal>
  );
}
