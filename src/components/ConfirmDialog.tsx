import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Modal } from '@/components/ui/modal';

interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel: string;
  /** Action destructrice : bouton rouge. */
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Demande de confirmation avant une action irréversible. Le focus part sur « Annuler ». */
export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  destructive = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <Modal
      labelledBy={`${id}-title`}
      describedBy={`${id}-message`}
      onClose={onCancel}
      role="alertdialog"
      scope="window"
    >
      <h2 id={`${id}-title`} className="text-lg font-semibold">
        {title}
      </h2>
      <p id={`${id}-message`} className="text-sm whitespace-pre-line">
        {message}
      </p>
      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={onCancel} data-autofocus>
          {t('common.cancel')}
        </Button>
        <Button variant={destructive ? 'destructive' : 'default'} onClick={onConfirm}>
          {confirmLabel}
        </Button>
      </div>
    </Modal>
  );
}
