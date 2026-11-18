import { LoaderCircle, RotateCw, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { HostKeyChangedAlert } from '@/features/connect/HostKeyChangedAlert';
import { HostKeyDialog } from '@/features/connect/HostKeyDialog';
import { QuickConnectForm } from '@/features/connect/QuickConnectForm';
import { cn } from '@/lib/utils';
import { type SessionTab, useSessions } from '@/stores/sessions';

import { closeTab, terminalSession } from './TerminalSession';
import { TerminalView } from './TerminalView';

/** Contenu d'un onglet de session : terminal et écran adapté à l'état de la connexion. */
export function SessionPanel({ tab, active }: { tab: SessionTab; active: boolean }) {
  const { t } = useTranslation();
  const update = useSessions((s) => s.update);
  const target = tab.target;

  return (
    <div
      role="tabpanel"
      id={`panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!active}
      className={cn('relative min-h-0 flex-1', !active && 'hidden')}
    >
      <TerminalView tabId={tab.id} active={active} interactive={tab.status === 'connected'} />

      {tab.status === 'form' && (
        <div className="absolute inset-0 z-20 flex items-start justify-center overflow-auto bg-background/95 p-8">
          <QuickConnectForm
            // Nouvelle instance à chaque retour au formulaire : valeurs pré-remplies à jour.
            key={`${tab.errorCode ?? ''}-${target?.host ?? ''}`}
            initial={target}
            errorCode={tab.errorCode}
            onSubmit={(next, secrets) => {
              void terminalSession(tab.id).connect(next, secrets);
            }}
          />
        </div>
      )}

      {(tab.status === 'connecting' || tab.status === 'authenticating') && target && (
        <div
          role="status"
          className="absolute top-3 left-1/2 z-20 flex -translate-x-1/2 items-center gap-2 rounded-md border bg-background px-4 py-2 text-sm shadow-md"
        >
          <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
          {tab.status === 'connecting'
            ? t('session.connecting', { host: target.host })
            : t('session.authenticating')}
        </div>
      )}

      {tab.status === 'hostKey' && tab.pendingHostKey && target && (
        <HostKeyDialog
          host={target.host}
          port={target.port}
          hostKey={tab.pendingHostKey}
          onDecision={(accept) => void terminalSession(tab.id).decideHostKey(accept)}
        />
      )}

      {tab.status === 'blocked' && tab.hostKeyChange && target && (
        <div className="absolute inset-0 z-20 flex items-start justify-center overflow-auto bg-background/95 p-8">
          <HostKeyChangedAlert
            host={target.host}
            port={target.port}
            change={tab.hostKeyChange}
            onClose={() => {
              closeTab(tab.id);
            }}
          />
        </div>
      )}

      {tab.status === 'disconnected' && (
        <div className="absolute inset-x-4 bottom-4 z-20 flex flex-wrap items-center justify-between gap-3 rounded-md border bg-background p-3 shadow-lg">
          <ErrorMessage code={tab.errorCode ?? 'ssh.connection_lost'} />
          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={() => {
                update(tab.id, { status: 'form', errorCode: null });
              }}
            >
              <RotateCw aria-hidden="true" />
              {t('session.reconnect')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                closeTab(tab.id);
              }}
            >
              <X aria-hidden="true" />
              {t('session.closeTab')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
