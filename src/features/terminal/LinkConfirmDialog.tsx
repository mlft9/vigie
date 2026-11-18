import { ExternalLink } from 'lucide-react';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Modal } from '@/components/ui/modal';

interface LinkConfirmDialogProps {
  url: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Confirmation d'un lien OSC 8 : le texte affiché dans le terminal peut différer de la cible,
 * on montre donc l'adresse réelle avant de l'ouvrir.
 */
export function LinkConfirmDialog({ url, onConfirm, onCancel }: LinkConfirmDialogProps) {
  const { t } = useTranslation();
  const id = useId();

  return (
    <Modal labelledBy={`${id}-title`} describedBy={`${id}-description`} onClose={onCancel}>
      <h2 id={`${id}-title`} className="flex items-center gap-2 text-lg font-semibold">
        <ExternalLink aria-hidden="true" className="size-5" />
        {t('link.title')}
      </h2>
      <p id={`${id}-description`} className="text-sm">
        {t('link.description')}
      </p>
      <p className="rounded-md border bg-muted/50 p-3 font-mono text-xs break-all select-all">
        {url}
      </p>
      <div className="flex justify-end gap-2">
        <Button variant="outline" data-autofocus onClick={onCancel}>
          {t('common.cancel')}
        </Button>
        <Button onClick={onConfirm}>{t('link.open')}</Button>
      </div>
    </Modal>
  );
}
