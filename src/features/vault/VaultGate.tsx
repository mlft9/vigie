import { LoaderCircle, RotateCw } from 'lucide-react';
import { type ReactNode, useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { useVault } from '@/stores/vault';

import { CreateVaultForm } from './CreateVaultForm';
import { UnlockForm } from './UnlockForm';
import { useVaultSession } from './useVaultSession';

/**
 * Porte du coffre : rien de l'application (carnet, terminaux, connexion rapide) n'est affiché
 * tant que le coffre n'est pas déverrouillé. Au premier lancement, elle propose de le créer.
 */
export function VaultGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const status = useVault((s) => s.status);
  const statusError = useVault((s) => s.statusError);
  const reason = useVault((s) => s.lastLockReason);
  const refreshStatus = useVault((s) => s.refreshStatus);
  useVaultSession();

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  if (status === 'unlocked') return children;

  return (
    <main
      data-testid="vault-gate"
      className="flex h-full items-start justify-center overflow-auto bg-muted/30 p-8 pt-[12vh]"
    >
      {status === 'missing' && <CreateVaultForm onCreated={() => void refreshStatus()} />}
      {status === 'locked' && (
        <UnlockForm reason={reason} onUnlocked={() => void refreshStatus()} />
      )}
      {status === null && statusError === null && (
        <p role="status" className="flex items-center gap-2 text-sm text-muted-foreground">
          <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
          {t('vault.loading')}
        </p>
      )}
      {status === null && statusError !== null && (
        <div className="flex max-w-md flex-col gap-3">
          <ErrorMessage code={statusError} />
          <Button variant="outline" className="self-end" onClick={() => void refreshStatus()}>
            <RotateCw aria-hidden="true" />
            {t('common.retry')}
          </Button>
        </div>
      )}
    </main>
  );
}
