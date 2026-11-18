// Ouverture d'un client SFTP sur la connexion d'une session de terminal (SFTP-01, SFTP-02).

import { useCallback, useEffect, useState } from 'react';

import { sftpClose, sftpOpen, toAppError } from '@/lib/ipc';
import type { SftpId } from '@/lib/types';
import { useSftp } from '@/stores/sftp';

export interface SftpClientState {
  sftpId: SftpId | null;
  /** Code d'erreur de la dernière ouverture. */
  errorCode: string | null;
  opening: boolean;
  retry: () => void;
}

interface OpenResult {
  /** Tentative à laquelle ce résultat correspond (session et numéro d'essai). */
  key: string;
  sftpId: SftpId | null;
  errorCode: string | null;
}

/**
 * Ouvre un client SFTP tant que `sessionId` est défini (session connectée) et le ferme au
 * démontage ou quand la session se termine. Les événements du client (transferts, éditions)
 * alimentent le store `useSftp`.
 */
export function useSftpClient(sessionId: number | null): SftpClientState {
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<OpenResult | null>(null);
  const key = `${String(sessionId)}:${String(attempt)}`;

  useEffect(() => {
    if (sessionId === null) return;
    let cancelled = false;
    let opened: SftpId | null = null;
    sftpOpen(sessionId, (event) => {
      if (opened !== null) useSftp.getState().handleEvent(opened, event);
    })
      .then((id) => {
        if (cancelled) {
          void sftpClose(id).catch(() => undefined);
          return;
        }
        opened = id;
        setResult({ key, sftpId: id, errorCode: null });
      })
      .catch((error: unknown) => {
        if (!cancelled) setResult({ key, sftpId: null, errorCode: toAppError(error).code });
      });
    return () => {
      cancelled = true;
      if (opened !== null) {
        const id = opened;
        void sftpClose(id).catch(() => undefined);
        useSftp.getState().forget(id);
      }
    };
  }, [sessionId, key]);

  const retry = useCallback(() => {
    setAttempt((n) => n + 1);
  }, []);

  const current = sessionId !== null && result?.key === key ? result : null;
  return {
    sftpId: current?.sftpId ?? null,
    errorCode: current?.errorCode ?? null,
    opening: sessionId !== null && current === null,
    retry,
  };
}
