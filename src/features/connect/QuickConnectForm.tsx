import { FileKey, KeyRound, LoaderCircle } from 'lucide-react';
import { type SyntheticEvent, useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { sshPickKeyFile, toAppError } from '@/lib/ipc';
import type { KeyFileRef } from '@/lib/types';
import { cn } from '@/lib/utils';
import type { ConnectTarget } from '@/stores/sessions';

import type { ConnectSecrets } from '../terminal/TerminalSession';
import { ErrorMessage } from './ErrorMessage';

const PASSPHRASE_ERRORS = new Set(['ssh.key_passphrase_required', 'ssh.key_wrong_passphrase']);
const KEY_FILE_ERRORS = new Set(['ssh.key_file_unknown', 'ssh.key_invalid', 'ssh.key_read_failed']);

interface QuickConnectFormProps {
  /** Valeurs de départ (reconnexion) : jamais de secret. */
  initial: ConnectTarget | null;
  /** Code d'erreur de la tentative précédente. */
  errorCode: string | null;
  onSubmit: (target: ConnectTarget, secrets: ConnectSecrets) => void;
}

/**
 * Formulaire de connexion rapide (SSH-01) : hôte, port, utilisateur, et mot de passe, clé ou
 * questions du serveur (keyboard-interactive, AUTH-03). Quel que soit le choix, un serveur
 * qui exige un second facteur (code TOTP) le demande ensuite dans une fenêtre dédiée.
 */
export function QuickConnectForm({ initial, errorCode, onSubmit }: QuickConnectFormProps) {
  const { t } = useTranslation();
  const id = useId();
  const [host, setHost] = useState(initial?.host ?? '');
  const [port, setPort] = useState(String(initial?.port ?? 22));
  const [username, setUsername] = useState(initial?.username ?? '');
  const [authKind, setAuthKind] = useState<ConnectTarget['authKind']>(
    initial?.authKind ?? 'password',
  );
  const [keyFile, setKeyFile] = useState<KeyFileRef | null>(
    KEY_FILE_ERRORS.has(errorCode ?? '') ? null : (initial?.keyFile ?? null),
  );
  // Secrets : état local uniquement, vidé à chaque envoi.
  const [password, setPassword] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [localError, setLocalError] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);

  const hostRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);
  const passphraseRef = useRef<HTMLInputElement>(null);

  // Place le curseur là où l'utilisateur doit agir.
  useEffect(() => {
    if (PASSPHRASE_ERRORS.has(errorCode ?? '')) {
      passphraseRef.current?.focus();
    } else if (initial && authKind === 'password') {
      passwordRef.current?.focus();
    } else {
      hostRef.current?.focus();
    }
    // Uniquement à l'affichage du formulaire.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const portNumber = Number(port);
  const portValid = /^\d+$/.test(port) && portNumber >= 1 && portNumber <= 65535;

  async function pickKey() {
    setPicking(true);
    setLocalError(null);
    try {
      const picked = await sshPickKeyFile();
      if (picked) setKeyFile(picked);
    } catch (error) {
      setLocalError(toAppError(error).code);
    } finally {
      setPicking(false);
    }
  }

  function submit(event: SyntheticEvent) {
    event.preventDefault();
    if (host.trim() === '' || username === '') {
      setLocalError('connect.missing_fields');
      return;
    }
    if (!portValid) {
      setLocalError('ssh.invalid_port');
      return;
    }
    if (authKind === 'password' && password === '') {
      setLocalError('connect.missing_password');
      return;
    }
    if (authKind === 'key' && !keyFile) {
      setLocalError('connect.missing_key');
      return;
    }
    const target: ConnectTarget = {
      host: host.trim(),
      port: portNumber,
      username,
      authKind,
      keyFile: authKind === 'key' ? keyFile : null,
    };
    const secrets: ConnectSecrets =
      authKind === 'password' ? { password } : passphrase ? { passphrase } : {};
    setPassword('');
    setPassphrase('');
    setLocalError(null);
    onSubmit(target, secrets);
  }

  const shownError = localError ?? errorCode;

  return (
    <form
      onSubmit={submit}
      noValidate
      aria-labelledby={`${id}-title`}
      className="flex w-full max-w-md flex-col gap-4 rounded-lg border bg-background p-6 shadow-sm"
    >
      <h2 id={`${id}-title`} className="text-lg font-semibold">
        {initial ? t('connect.reconnectTitle') : t('connect.title')}
      </h2>

      {shownError && <ErrorMessage code={shownError} />}

      <div className="grid grid-cols-[1fr_6rem] gap-3">
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-host`} className="text-sm font-medium">
            {t('connect.host')}
          </label>
          <Input
            ref={hostRef}
            id={`${id}-host`}
            value={host}
            onChange={(e) => {
              setHost(e.target.value);
            }}
            placeholder={t('connect.hostPlaceholder')}
            autoComplete="off"
            spellCheck={false}
            maxLength={255}
            required
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-port`} className="text-sm font-medium">
            {t('connect.port')}
          </label>
          <Input
            id={`${id}-port`}
            value={port}
            onChange={(e) => {
              setPort(e.target.value);
            }}
            inputMode="numeric"
            aria-invalid={!portValid}
            maxLength={5}
            required
          />
        </div>
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
          autoComplete="off"
          spellCheck={false}
          maxLength={256}
          required
        />
      </div>

      <fieldset className="flex flex-col gap-2">
        <legend className="mb-1.5 text-sm font-medium">{t('connect.authMethod')}</legend>
        <div className="flex flex-wrap gap-x-4 gap-y-2">
          {(['password', 'key', 'interactive'] as const).map((kind) => (
            <label key={kind} className="flex items-center gap-2 text-sm">
              <input
                type="radio"
                name={`${id}-auth`}
                value={kind}
                checked={authKind === kind}
                onChange={() => {
                  setAuthKind(kind);
                  setLocalError(null);
                }}
                className="accent-primary"
              />
              {t(`connect.auth.${kind}`)}
            </label>
          ))}
        </div>
      </fieldset>

      {authKind === 'interactive' && (
        <p className="text-sm text-muted-foreground">{t('connect.interactiveHint')}</p>
      )}

      {authKind === 'password' && (
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-password`} className="text-sm font-medium">
            {t('connect.password')}
          </label>
          <Input
            ref={passwordRef}
            id={`${id}-password`}
            type="password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
            }}
            autoComplete="off"
            maxLength={1024}
          />
        </div>
      )}

      {authKind === 'key' && (
        <>
          <div className="flex flex-col gap-1.5">
            <span className="text-sm font-medium">{t('connect.keyFile')}</span>
            <div className="flex items-center gap-3">
              <Button
                type="button"
                variant="outline"
                onClick={() => void pickKey()}
                disabled={picking}
              >
                {picking ? (
                  <LoaderCircle aria-hidden="true" className="animate-spin" />
                ) : (
                  <FileKey aria-hidden="true" />
                )}
                {t('connect.chooseKey')}
              </Button>
              <span
                className={cn('truncate text-sm', !keyFile && 'text-muted-foreground')}
                data-testid="key-file-name"
              >
                {keyFile ? keyFile.name : t('connect.noKey')}
              </span>
            </div>
          </div>
          <div className="flex flex-col gap-1.5">
            <label htmlFor={`${id}-passphrase`} className="text-sm font-medium">
              {t('connect.passphrase')}
            </label>
            <Input
              ref={passphraseRef}
              id={`${id}-passphrase`}
              type="password"
              value={passphrase}
              onChange={(e) => {
                setPassphrase(e.target.value);
              }}
              autoComplete="off"
              maxLength={1024}
              aria-describedby={`${id}-passphrase-hint`}
            />
            <p id={`${id}-passphrase-hint`} className="text-xs text-muted-foreground">
              {t('connect.passphraseHint')}
            </p>
          </div>
        </>
      )}

      <Button type="submit" className="self-end">
        <KeyRound aria-hidden="true" />
        {t('connect.submit')}
      </Button>
    </form>
  );
}
