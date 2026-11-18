import type { SftpId } from '@/lib/types';
import { type Transfer, useSftp } from '@/stores/sftp';

/** Premier transfert de ce client en attente d'une décision de conflit. */
export function usePendingConflict(sftpId: SftpId | null): Transfer | undefined {
  return useSftp((s) =>
    sftpId === null
      ? undefined
      : s.transfers.find((t) => t.sftpId === sftpId && t.status === 'conflict'),
  );
}
