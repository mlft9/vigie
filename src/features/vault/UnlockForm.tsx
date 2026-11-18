import { LoaderCircle, LockKeyhole, LockOpen } from 'lucide-react';
import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { toAppError, vaultUnlock } from '@/lib/ipc';
import type { LockReason } from '@/lib/types';

interface UnlockFormProps {
  /** Cause du verrouillage précédent, expliquée à l'utilisateur. */
  reason: LockReason | null;
  onUnlocked: () => void;
}

/**
 * Déverrouillage du coffre. Un échec affiche toujours le même message, sans indiquer si le
 * mot de passe est faux ou le fichier abîmé. Le délai après plusieurs échecs est imposé par
 * le cœur : ici, on se contente d'afficher l'attente.
 */
export function UnlockForm({ reason, onUnlocked }: UnlockFormProps) {
  const { t } = useTranslation();
  const id = useId();
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    if (password === '') return;
    const secret = password;
    setPassword('');
    setError(null);
    setBusy(true);
    try {
      await vaultUnlock(secret);
      onUnlocked();
    } catch (failure) {
      setError(toAppError(failure).code);
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      onSubmit={(event) => void submit(event)}
      noValidate
      aria-labelledby={`${id}-title`}
      className="flex w-full max-w-sm flex-col gap-4 rounded-lg border bg-background p-6 shadow-sm"
    >
      <h1 id={`${id}-title`} className="flex items-center gap-2 text-lg font-semibold">
        <LockKeyhole aria-hidden="true" className="size-5" />
        {t('vault.unlock.title')}
      </h1>
      {reason && reason !== 'manual' && (
        <p className="text-sm text-muted-foreground">{t(`vault.unlock.reason.${reason}`)}</p>
      )}
      {error && <ErrorMessage code={error} />}
      <div className="flex flex-col gap-1.5">
        <label htmlFor={`${id}-password`} className="text-sm font-medium">
          {t('vault.unlock.password')}
        </label>
        <Input
          id={`${id}-password`}
          type="password"
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
          }}
          autoComplete="current-password"
          maxLength={1024}
          disabled={busy}
          autoFocus
        />
      </div>
      <Button type="submit" className="self-end" disabled={busy || password === ''}>
        {busy ? (
          <LoaderCircle aria-hidden="true" className="animate-spin" />
        ) : (
          <LockOpen aria-hidden="true" />
        )}
        {busy ? t('vault.unlock.unlocking') : t('vault.unlock.submit')}
      </Button>
    </form>
  );
}
