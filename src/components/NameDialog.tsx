import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { toAppError } from '@/lib/ipc';

/** Longueur maximale d'un nom (revérifiée par le cœur). */
export const MAX_NAME_LENGTH = 128;

interface NameDialogProps {
  title: string;
  label: string;
  initial: string;
  /** Longueur maximale (par défaut `MAX_NAME_LENGTH`). */
  maxLength?: number;
  /** Enregistre le nom ; une erreur du cœur est affichée dans la fenêtre. */
  onSave: (name: string) => Promise<void>;
  onClose: () => void;
}

/** Saisie d'un nom (groupe, clé). */
export function NameDialog({
  title,
  label,
  initial,
  maxLength = MAX_NAME_LENGTH,
  onSave,
  onClose,
}: NameDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const [name, setName] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    if (name.trim() === '') {
      setError('vault.invalid_name');
      return;
    }
    setBusy(true);
    try {
      await onSave(name.trim());
      onClose();
    } catch (failure) {
      setError(toAppError(failure).code);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal labelledBy={`${id}-title`} onClose={onClose} scope="window">
      <form onSubmit={(event) => void submit(event)} noValidate className="flex flex-col gap-4">
        <h2 id={`${id}-title`} className="text-lg font-semibold">
          {title}
        </h2>
        {error && <ErrorMessage code={error} />}
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-name`} className="text-sm font-medium">
            {label}
          </label>
          <Input
            id={`${id}-name`}
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            maxLength={maxLength}
            autoComplete="off"
            data-autofocus
          />
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" disabled={busy}>
            {t('common.save')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
