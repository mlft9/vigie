import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import { NativeSelect } from '@/components/ui/native-select';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { identitySave, toAppError } from '@/lib/ipc';
import type { IdentityEntry, KeyEntry, PasswordRequest } from '@/lib/types';

interface IdentityFormProps {
  identity: IdentityEntry | null;
  keys: readonly KeyEntry[];
  onSaved: () => void;
  onClose: () => void;
}

/**
 * Identité réutilisable (HST-03) : utilisateur, mot de passe et/ou clé du coffre. Le mot de
 * passe enregistré n'est jamais renvoyé par le cœur : on peut le garder, le remplacer ou le
 * retirer, pas le lire.
 */
export function IdentityForm({ identity, keys, onSaved, onClose }: IdentityFormProps) {
  const { t } = useTranslation();
  const id = useId();
  const [name, setName] = useState(identity?.name ?? '');
  const [username, setUsername] = useState(identity?.username ?? '');
  const [keyId, setKeyId] = useState<string>(identity?.keyId ?? '');
  const hadPassword = identity?.hasPassword ?? false;
  const [removePassword, setRemovePassword] = useState(false);
  // Secret : état local uniquement, vidé à l'envoi.
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: SyntheticEvent) {
    event.preventDefault();
    let change: PasswordRequest;
    if (password !== '') change = { action: 'set', value: password };
    else if (hadPassword && !removePassword) change = { action: 'keep' };
    else change = { action: 'clear' };
    if (change.action === 'clear' && keyId === '') {
      setError('vault.identity_incomplete');
      return;
    }
    setPassword('');
    setBusy(true);
    setError(null);
    try {
      await identitySave({
        id: identity?.id ?? null,
        name: name.trim() === '' ? username : name.trim(),
        username,
        password: change,
        keyId: keyId === '' ? null : keyId,
      });
      onSaved();
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
          {identity ? t('identities.editTitle') : t('identities.newTitle')}
        </h2>
        <p className="text-sm text-muted-foreground">{t('identities.intro')}</p>
        {error && <ErrorMessage code={error} />}
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-name`} className="text-sm font-medium">
            {t('identities.name')}
          </label>
          <Input
            id={`${id}-name`}
            value={name}
            onChange={(e) => {
              setName(e.target.value);
            }}
            maxLength={128}
            autoComplete="off"
            data-autofocus
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-user`} className="text-sm font-medium">
            {t('connect.username')}
          </label>
          <Input
            id={`${id}-user`}
            value={username}
            onChange={(e) => {
              setUsername(e.target.value);
            }}
            maxLength={256}
            autoComplete="off"
            spellCheck={false}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-key`} className="text-sm font-medium">
            {t('identities.key')}
          </label>
          <NativeSelect
            id={`${id}-key`}
            value={keyId}
            onChange={(e) => {
              setKeyId(e.target.value);
            }}
          >
            <option value="">{t('identities.noKey')}</option>
            {keys.map((key) => (
              <option key={key.id} value={key.id}>
                {key.name} ({key.algorithm})
              </option>
            ))}
          </NativeSelect>
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-password`} className="text-sm font-medium">
            {hadPassword ? t('identities.newPassword') : t('connect.password')}
          </label>
          <Input
            id={`${id}-password`}
            type="password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
            }}
            maxLength={1024}
            autoComplete="off"
            aria-describedby={`${id}-password-hint`}
          />
          <p id={`${id}-password-hint`} className="text-xs text-muted-foreground">
            {hadPassword ? t('identities.passwordKeptHint') : t('identities.passwordHint')}
          </p>
          {hadPassword && (
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={removePassword}
                onChange={(e) => {
                  setRemovePassword(e.target.checked);
                }}
                className="accent-primary"
              />
              {t('identities.removePassword')}
            </label>
          )}
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
