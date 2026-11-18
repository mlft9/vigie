import { LoaderCircle } from 'lucide-react';
import { type SyntheticEvent, useCallback, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { PasswordStrength } from '@/features/vault/PasswordStrength';
import {
  characterCount,
  MIN_MASTER_LENGTH,
  meetsPolicy,
  type StrengthScore,
} from '@/features/vault/strength';
import { toAppError, vaultChangePassword } from '@/lib/ipc';

/**
 * Changement du mot de passe maître (VLT-04). Le cœur vérifie l'actuel, applique la politique
 * au nouveau, puis renouvelle le sel et la DEK : l'ancien mot de passe ne fonctionne plus.
 */
export function ChangePasswordForm() {
  const { t } = useTranslation();
  const id = useId();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const [score, setScore] = useState<StrengthScore>(0);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);
  const onScore = useCallback((value: StrengthScore) => {
    setScore(value);
  }, []);

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    setDone(false);
    if (current === '') {
      setError('vault.current_password_required');
      return;
    }
    if (characterCount(next) < MIN_MASTER_LENGTH) {
      setError('vault.password_too_short');
      return;
    }
    if (!meetsPolicy(next, score)) {
      setError('vault.password_too_weak');
      return;
    }
    if (next !== confirm) {
      setError('vault.password_mismatch');
      return;
    }
    const secrets = { current, next };
    setCurrent('');
    setNext('');
    setConfirm('');
    setError(null);
    setBusy(true);
    try {
      await vaultChangePassword(secrets.current, secrets.next);
      setDone(true);
    } catch (failure) {
      setError(toAppError(failure).code);
    } finally {
      setBusy(false);
    }
  }

  const input = (
    suffix: string,
    label: string,
    value: string,
    set: (value: string) => void,
    autoComplete: string,
    describedBy?: string,
  ) => (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={`${id}-${suffix}`} className="text-sm font-medium">
        {label}
      </label>
      <Input
        id={`${id}-${suffix}`}
        type="password"
        value={value}
        onChange={(e) => {
          set(e.target.value);
        }}
        autoComplete={autoComplete}
        maxLength={1024}
        className="max-w-xs"
        aria-describedby={describedBy}
      />
    </div>
  );

  return (
    <form
      onSubmit={(event) => void submit(event)}
      noValidate
      aria-labelledby={`${id}-title`}
      className="flex flex-col gap-3"
    >
      <h3 id={`${id}-title`} className="text-sm font-medium">
        {t('settings.security.changePassword')}
      </h3>
      {error && <ErrorMessage code={error} />}
      {done && (
        <p role="status" className="text-sm text-emerald-700 dark:text-emerald-400">
          {t('settings.security.passwordChanged')}
        </p>
      )}
      {input('current', t('settings.security.current'), current, setCurrent, 'current-password')}
      {input('next', t('settings.security.new'), next, setNext, 'new-password', `${id}-strength`)}
      <div className="max-w-xs">
        <PasswordStrength id={`${id}-strength`} password={next} onScore={onScore} />
      </div>
      {input('confirm', t('vault.create.confirm'), confirm, setConfirm, 'new-password')}
      <Button type="submit" variant="outline" className="self-start" disabled={busy}>
        {busy && <LoaderCircle aria-hidden="true" className="animate-spin" />}
        {t('settings.security.changePasswordSubmit')}
      </Button>
    </form>
  );
}
