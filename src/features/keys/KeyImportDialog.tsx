import { FileKey, LoaderCircle } from 'lucide-react';
import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { keyImport, sshPickKeyFile, toAppError } from '@/lib/ipc';
import type { KeyFileRef } from '@/lib/types';
import { cn } from '@/lib/utils';

interface KeyImportDialogProps {
  onImported: () => void;
  onClose: () => void;
}

/**
 * Import d'une clé privée dans le coffre (KEY-02). Le fichier est choisi dans la boîte de
 * dialogue du système, ouverte par le cœur : l'interface ne voit ni le chemin ni la clé. La
 * passphrase sert une seule fois à déchiffrer la clé, qui est ensuite protégée par le coffre.
 */
export function KeyImportDialog({ onImported, onClose }: KeyImportDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const [file, setFile] = useState<KeyFileRef | null>(null);
  const [name, setName] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function pick() {
    setError(null);
    try {
      const picked = await sshPickKeyFile();
      if (picked) {
        setFile(picked);
        if (name === '') setName(picked.name);
      }
    } catch (failure) {
      setError(toAppError(failure).code);
    }
  }

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    if (!file) {
      setError('connect.missing_key');
      return;
    }
    const secret = passphrase;
    setPassphrase('');
    setBusy(true);
    setError(null);
    try {
      await keyImport({
        keyFileId: file.id,
        name: name.trim() === '' ? file.name : name.trim(),
        passphrase: secret === '' ? null : secret,
      });
      onImported();
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
          {t('keys.import.title')}
        </h2>
        <p className="text-sm text-muted-foreground">{t('keys.import.intro')}</p>
        {error && <ErrorMessage code={error} />}
        <div className="flex items-center gap-3">
          <Button type="button" variant="outline" onClick={() => void pick()} data-autofocus>
            <FileKey aria-hidden="true" />
            {t('connect.chooseKey')}
          </Button>
          <span
            className={cn('truncate text-sm', !file && 'text-muted-foreground')}
            data-testid="import-file-name"
          >
            {file ? file.name : t('connect.noKey')}
          </span>
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-name`} className="text-sm font-medium">
            {t('keys.name')}
          </label>
          <Input
            id={`${id}-name`}
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            maxLength={128}
            autoComplete="off"
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-passphrase`} className="text-sm font-medium">
            {t('connect.passphrase')}
          </label>
          <Input
            id={`${id}-passphrase`}
            type="password"
            value={passphrase}
            onChange={(e) => {
              setPassphrase(e.target.value);
            }}
            maxLength={1024}
            autoComplete="off"
            aria-describedby={`${id}-hint`}
          />
          <p id={`${id}-hint`} className="text-xs text-muted-foreground">
            {t('keys.import.passphraseHint')}
          </p>
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" disabled={busy || !file}>
            {busy && <LoaderCircle aria-hidden="true" className="animate-spin" />}
            {t('keys.import.submit')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
