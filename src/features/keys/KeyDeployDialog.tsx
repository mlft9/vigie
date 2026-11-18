import { CircleCheck, LoaderCircle, ShieldX, Upload } from 'lucide-react';
import { type SyntheticEvent, useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import { NativeSelect } from '@/components/ui/native-select';
import { AuthPromptDialog } from '@/features/connect/AuthPromptDialog';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { HostKeyDialog } from '@/features/connect/HostKeyDialog';
import { keyDeploy, sshAuthAnswer, sshClose, sshHostKeyDecision, toAppError } from '@/lib/ipc';
import type {
  AuthPromptInfo,
  HostEntry,
  HostKeyInfo,
  KeyEntry,
  SessionEvent,
  SessionId,
} from '@/lib/types';

type Phase =
  | { kind: 'choose' }
  | { kind: 'running' }
  | { kind: 'hostKey'; key: HostKeyInfo }
  | { kind: 'prompt'; prompt: AuthPromptInfo; errorCode: string | null }
  | { kind: 'blocked' }
  | { kind: 'done'; added: boolean };

interface KeyDeployDialogProps {
  keyEntry: KeyEntry;
  hosts: readonly HostEntry[];
  onClose: () => void;
  /** Ouvre la gestion des clés d'hôte (clé modifiée). */
  onManageHostKeys: () => void;
}

/**
 * Déploiement d'une clé du coffre sur un hôte du carnet (KEY-03). Le cœur se connecte avec
 * l'identité de l'hôte (même vérification de clé d'hôte, même 2FA qu'une session), puis ajoute
 * la clé publique à `~/.ssh/authorized_keys` sans doublon, avec des permissions 700/600.
 * Seuls les identifiants de l'hôte et de la clé quittent l'interface.
 */
export function KeyDeployDialog({
  keyEntry,
  hosts,
  onClose,
  onManageHostKeys,
}: KeyDeployDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const [hostId, setHostId] = useState(hosts[0]?.id ?? '');
  const [password, setPassword] = useState('');
  const [phase, setPhase] = useState<Phase>({ kind: 'choose' });
  const [error, setError] = useState<string | null>(null);
  const session = useRef<SessionId | null>(null);
  const deployed = useRef(false);
  const blocked = useRef(false);

  const host = hosts.find((h) => h.id === hostId) ?? null;
  const address = host ? (host.port === 22 ? host.address : `${host.address}:${host.port}`) : '';

  // Fermeture du dialogue pendant le déploiement : la session est abandonnée.
  useEffect(
    () => () => {
      if (session.current !== null) void sshClose(session.current).catch(() => undefined);
    },
    [],
  );

  function onEvent(event: SessionEvent) {
    switch (event.type) {
      case 'hostKeyUnknown':
        setPhase({ kind: 'hostKey', key: event.key });
        break;
      case 'hostKeyChanged':
        blocked.current = true;
        setPhase({ kind: 'blocked' });
        break;
      case 'authPrompt':
        setPhase({
          kind: 'prompt',
          prompt: {
            round: event.round,
            name: event.name,
            instructions: event.instructions,
            prompts: event.prompts,
          },
          errorCode: null,
        });
        break;
      case 'authenticating':
      case 'connected':
        if (!blocked.current) setPhase({ kind: 'running' });
        break;
      case 'keyDeployed':
        deployed.current = true;
        setPhase({ kind: 'done', added: event.added });
        break;
      case 'disconnected':
        session.current = null;
        if (deployed.current || blocked.current || event.code === 'ssh.session_closed') return;
        setError(event.code);
        setPhase({ kind: 'choose' });
        break;
    }
  }

  async function start(event: SyntheticEvent) {
    event.preventDefault();
    if (!host) return;
    const secret = host.identityId === null && password !== '' ? password : null;
    setPassword('');
    setError(null);
    deployed.current = false;
    blocked.current = false;
    setPhase({ kind: 'running' });
    try {
      session.current = await keyDeploy(
        { hostId: host.id, keyId: keyEntry.id, password: secret },
        onEvent,
      );
    } catch (failure) {
      setError(toAppError(failure).code);
      setPhase({ kind: 'choose' });
    }
  }

  function decideHostKey(accept: boolean) {
    setPhase({ kind: 'running' });
    if (session.current !== null) {
      void sshHostKeyDecision(session.current, accept).catch(() => undefined);
    }
  }

  function answer(prompt: AuthPromptInfo, answers: string[] | null) {
    setPhase({ kind: 'running' });
    if (session.current === null) return;
    sshAuthAnswer(session.current, prompt.round, answers).catch((failure: unknown) => {
      const code = toAppError(failure).code;
      // Réponses refusées par le cœur : la question est toujours en attente, on la réaffiche.
      if (code === 'ssh.invalid_answers' && session.current !== null) {
        setPhase({ kind: 'prompt', prompt, errorCode: code });
      }
    });
  }

  function cancel() {
    if (session.current !== null) void sshClose(session.current).catch(() => undefined);
    session.current = null;
    onClose();
  }

  if (phase.kind === 'hostKey' && host) {
    return (
      <HostKeyDialog
        host={host.address}
        port={host.port}
        hostKey={phase.key}
        onDecision={decideHostKey}
      />
    );
  }

  if (phase.kind === 'prompt') {
    return (
      <AuthPromptDialog
        key={JSON.stringify(phase.prompt)}
        address={address}
        prompt={phase.prompt}
        errorCode={phase.errorCode}
        onSubmit={(answers) => {
          answer(phase.prompt, answers);
        }}
        onCancel={() => {
          answer(phase.prompt, null);
        }}
      />
    );
  }

  return (
    <Modal labelledBy={`${id}-title`} onClose={cancel} scope="window">
      <div className="flex flex-col gap-4">
        <h2 id={`${id}-title`} className="text-lg font-semibold">
          {t('keys.deploy.title', { name: keyEntry.name })}
        </h2>

        {phase.kind === 'choose' && (
          <form onSubmit={(event) => void start(event)} noValidate className="flex flex-col gap-4">
            <p className="text-sm text-muted-foreground">{t('keys.deploy.intro')}</p>
            {error && <ErrorMessage code={error} />}
            {hosts.length === 0 ? (
              <p className="text-sm">{t('keys.deploy.noHosts')}</p>
            ) : (
              <>
                <div className="flex flex-col gap-1.5">
                  <label htmlFor={`${id}-host`} className="text-sm font-medium">
                    {t('keys.deploy.host')}
                  </label>
                  <NativeSelect
                    id={`${id}-host`}
                    value={hostId}
                    onChange={(e) => {
                      setHostId(e.target.value);
                      setPassword('');
                    }}
                    data-autofocus
                  >
                    {hosts.map((h) => (
                      <option key={h.id} value={h.id}>
                        {h.name} ({h.address})
                      </option>
                    ))}
                  </NativeSelect>
                </div>
                {host?.identityId === null && (
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor={`${id}-password`} className="text-sm font-medium">
                      {t('connect.password')}
                    </label>
                    <Input
                      id={`${id}-password`}
                      type="password"
                      value={password}
                      onChange={(e) => {
                        setPassword(e.target.value);
                      }}
                      autoComplete="off"
                      maxLength={1024}
                      aria-describedby={`${id}-password-hint`}
                    />
                    <p id={`${id}-password-hint`} className="text-xs text-muted-foreground">
                      {t('keys.deploy.passwordHint')}
                    </p>
                  </div>
                )}
              </>
            )}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="outline" onClick={cancel}>
                {t('common.cancel')}
              </Button>
              <Button type="submit" disabled={!host}>
                <Upload aria-hidden="true" />
                {t('keys.deploy.submit')}
              </Button>
            </div>
          </form>
        )}

        {phase.kind === 'running' && (
          <>
            <p role="status" className="flex items-center gap-2 text-sm">
              <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
              {t('keys.deploy.running', { host: address })}
            </p>
            <div className="flex justify-end">
              <Button variant="outline" onClick={cancel}>
                {t('common.cancel')}
              </Button>
            </div>
          </>
        )}

        {phase.kind === 'blocked' && (
          <>
            <p role="alert" className="flex gap-2 text-sm">
              <ShieldX aria-hidden="true" className="size-5 shrink-0 text-destructive" />
              {t('keys.deploy.blocked', { host: address })}
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={onClose}>
                {t('common.close')}
              </Button>
              <Button
                onClick={() => {
                  onClose();
                  onManageHostKeys();
                }}
              >
                {t('hostKey.changed.manage')}
              </Button>
            </div>
          </>
        )}

        {phase.kind === 'done' && (
          <>
            <p role="status" className="flex gap-2 text-sm">
              <CircleCheck aria-hidden="true" className="size-5 shrink-0 text-primary" />
              {phase.added
                ? t('keys.deploy.added', { host: address })
                : t('keys.deploy.present', { host: address })}
            </p>
            <p className="text-sm text-muted-foreground">{t('keys.deploy.nextStep')}</p>
            <div className="flex justify-end">
              <Button onClick={onClose} data-autofocus>
                {t('common.close')}
              </Button>
            </div>
          </>
        )}
      </div>
    </Modal>
  );
}
