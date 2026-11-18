import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { toAppError } from '@/lib/ipc';

import { parseOctal } from './format';

const WHO = ['owner', 'group', 'others'] as const;
const WHAT = ['read', 'write', 'exec'] as const;

interface ChmodDialogProps {
  /** Nom affichable de l'élément. */
  name: string;
  /** Droits actuels (12 bits), ou `null` s'ils sont inconnus. */
  initial: number | null;
  onSave: (mode: number) => Promise<void>;
  onClose: () => void;
}

/** Modification des droits Unix d'un élément distant (cases rwx et saisie octale). */
export function ChmodDialog({ name, initial, onSave, onClose }: ChmodDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const [mode, setMode] = useState(initial ?? 0o644);
  const [text, setText] = useState((initial ?? 0o644).toString(8).padStart(3, '0'));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function toggle(bit: number) {
    const next = mode ^ bit;
    setMode(next);
    setText(next.toString(8).padStart(3, '0'));
  }

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    const value = parseOctal(text);
    if (value === null) {
      setError('ssh.sftp_invalid_mode');
      return;
    }
    setBusy(true);
    try {
      await onSave(value);
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
        <h2 id={`${id}-title`} className="text-lg font-semibold break-all">
          {t('sftp.chmodTitle', { name })}
        </h2>
        {error && <ErrorMessage code={error} />}
        <table className="text-sm">
          <thead>
            <tr>
              <th />
              {WHAT.map((what) => (
                <th key={what} scope="col" className="px-2 font-medium">
                  {t(`sftp.perm.${what}`)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {WHO.map((who, row) => (
              <tr key={who}>
                <th scope="row" className="pr-2 text-left font-medium">
                  {t(`sftp.perm.${who}`)}
                </th>
                {WHAT.map((what, col) => {
                  const bit = (4 >> col) << ((2 - row) * 3);
                  return (
                    <td key={what} className="px-2 text-center">
                      <input
                        type="checkbox"
                        aria-label={`${t(`sftp.perm.${who}`)} — ${t(`sftp.perm.${what}`)}`}
                        checked={(mode & bit) !== 0}
                        onChange={() => {
                          toggle(bit);
                        }}
                        className="size-4"
                      />
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-octal`} className="text-sm font-medium">
            {t('sftp.octal')}
          </label>
          <Input
            id={`${id}-octal`}
            value={text}
            inputMode="numeric"
            maxLength={4}
            autoComplete="off"
            onChange={(e) => {
              setText(e.target.value);
              const value = parseOctal(e.target.value);
              if (value !== null) setMode(value);
            }}
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
