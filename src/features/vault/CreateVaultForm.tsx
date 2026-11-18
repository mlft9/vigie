import { LoaderCircle, ShieldCheck, TriangleAlert } from 'lucide-react';
import { type SyntheticEvent, useCallback, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { toAppError, vaultCreate } from '@/lib/ipc';

import { PasswordStrength } from './PasswordStrength';
import { characterCount, MIN_MASTER_LENGTH, meetsPolicy, type StrengthScore } from './strength';

/** Création du coffre au premier lancement (VLT-01). */
export function CreateVaultForm({ onCreated }: { onCreated: () => void }) {
  const { t } = useTranslation();
  const id = useId();
  // Secrets : état local uniquement, vidé dès l'envoi.
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [score, setScore] = useState<StrengthScore>(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const onScore = useCallback((value: StrengthScore) => {
    setScore(value);
  }, []);

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    if (characterCount(password) < MIN_MASTER_LENGTH) {
      setError('vault.password_too_short');
      return;
    }
    if (!meetsPolicy(password, score)) {
      setError('vault.password_too_weak');
      return;
    }
    if (password !== confirm) {
      setError('vault.password_mismatch');
      return;
    }
    const secret = password;
    setPassword('');
    setConfirm('');
    setError(null);
    setBusy(true);
    try {
      await vaultCreate(secret);
      onCreated();
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
      className="flex w-full max-w-md flex-col gap-4 rounded-lg border bg-background p-6 shadow-sm"
    >
      <h1 id={`${id}-title`} className="flex items-center gap-2 text-lg font-semibold">
        <ShieldCheck aria-hidden="true" className="size-5" />
        {t('vault.create.title')}
      </h1>
      <p className="text-sm text-muted-foreground">{t('vault.create.intro')}</p>

      <p
        className="flex items-start gap-2 rounded-md border border-amber-500/50 bg-amber-500/10 px-3 py-2 text-sm"
        id={`${id}-warning`}
      >
        <TriangleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-amber-600" />
        <span>{t('vault.create.noRecovery')}</span>
      </p>

      {error && <ErrorMessage code={error} />}

      <div className="flex flex-col gap-1.5">
        <label htmlFor={`${id}-password`} className="text-sm font-medium">
          {t('vault.create.password')}
        </label>
        <Input
          id={`${id}-password`}
          type="password"
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
          }}
          autoComplete="new-password"
          maxLength={1024}
          aria-describedby={`${id}-strength ${id}-warning`}
          data-autofocus
          autoFocus
        />
        <PasswordStrength id={`${id}-strength`} password={password} onScore={onScore} />
      </div>

      <div className="flex flex-col gap-1.5">
        <label htmlFor={`${id}-confirm`} className="text-sm font-medium">
          {t('vault.create.confirm')}
        </label>
        <Input
          id={`${id}-confirm`}
          type="password"
          value={confirm}
          onChange={(e) => {
            setConfirm(e.target.value);
          }}
          autoComplete="new-password"
          maxLength={1024}
        />
      </div>

      <Button type="submit" className="self-end" disabled={busy}>
        {busy && <LoaderCircle aria-hidden="true" className="animate-spin" />}
        {busy ? t('vault.create.creating') : t('vault.create.submit')}
      </Button>
    </form>
  );
}
