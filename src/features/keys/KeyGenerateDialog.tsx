import { LoaderCircle } from 'lucide-react';
import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import { NativeSelect } from '@/components/ui/native-select';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { keyGenerate, toAppError } from '@/lib/ipc';
import type { KeyKind } from '@/lib/types';

/** Types proposés, Ed25519 en tête (choix par défaut). */
const KEY_KINDS: readonly KeyKind[] = ['ed25519', 'ecdsaP256', 'ecdsaP384', 'ecdsaP521', 'rsa4096'];

interface KeyGenerateDialogProps {
  onGenerated: () => void;
  onClose: () => void;
}

/**
 * Génération d'une clé (KEY-01). La clé naît dans le cœur et y reste, protégée par le coffre :
 * l'interface ne reçoit que la partie publique. Pas de passphrase : elle ne protégerait qu'un
 * fichier, or la clé privée n'est jamais écrite hors du coffre.
 */
export function KeyGenerateDialog({ onGenerated, onClose }: KeyGenerateDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const [kind, setKind] = useState<KeyKind>('ed25519');
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    const finalName =
      name.trim() === ''
        ? t('keys.generate.defaultName', { kind: t(`keys.kind.${kind}`) })
        : name.trim();
    setBusy(true);
    setError(null);
    try {
      await keyGenerate({ name: finalName, kind });
      onGenerated();
      onClose();
    } catch (failure) {
      setError(toAppError(failure).code);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      labelledBy={`${id}-title`}
      onClose={() => {
        if (!busy) onClose();
      }}
      scope="window"
    >
      <form onSubmit={(event) => void submit(event)} noValidate className="flex flex-col gap-4">
        <h2 id={`${id}-title`} className="text-lg font-semibold">
          {t('keys.generate.title')}
        </h2>
        <p className="text-sm text-muted-foreground">{t('keys.generate.intro')}</p>
        {error && <ErrorMessage code={error} />}
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-kind`} className="text-sm font-medium">
            {t('keys.generate.kind')}
          </label>
          <NativeSelect
            id={`${id}-kind`}
            value={kind}
            onChange={(e) => {
              setKind(e.target.value as KeyKind);
            }}
            disabled={busy}
            data-autofocus
          >
            {KEY_KINDS.map((option) => (
              <option key={option} value={option}>
                {t(`keys.kind.${option}`)}
              </option>
            ))}
          </NativeSelect>
          {kind === 'rsa4096' && (
            <p className="text-xs text-muted-foreground">{t('keys.generate.rsaHint')}</p>
          )}
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
            placeholder={t('keys.generate.defaultName', { kind: t(`keys.kind.${kind}`) })}
            maxLength={128}
            autoComplete="off"
            disabled={busy}
          />
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" disabled={busy}>
            {busy && <LoaderCircle aria-hidden="true" className="animate-spin" />}
            {busy ? t('keys.generate.busy') : t('keys.generate.submit')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
