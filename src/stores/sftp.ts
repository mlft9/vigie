import { create } from 'zustand';

import type { EditId, JobId, SftpEvent, SftpId } from '@/lib/types';

/** Nombre maximal de transferts terminés conservés dans la liste. */
const MAX_FINISHED = 50;

export type TransferStatus = 'queued' | 'running' | 'conflict' | 'done' | 'failed' | 'cancelled';

export interface TransferResult {
  transferred: number;
  skipped: number;
  ignored: number;
  refused: number;
  failed: number;
  code: string | null;
}

export interface Transfer {
  job: JobId;
  sftpId: SftpId;
  direction: 'download' | 'upload';
  /** Nom affichable du premier élément (nettoyé par le cœur). */
  label: string;
  count: number;
  status: TransferStatus;
  bytesDone: number;
  bytesTotal: number;
  filesDone: number;
  filesTotal: number;
  /** Nom de l'élément en conflit, en attente de décision. */
  conflictName: string | null;
  result: TransferResult | null;
}

export type EditStatus = 'watching' | 'uploaded' | 'conflict' | 'failed';

export interface EditInfo {
  edit: EditId;
  sftpId: SftpId;
  /** Nom affichable du fichier édité. */
  label: string;
  status: EditStatus;
  code: string | null;
}

interface SftpState {
  transfers: Transfer[];
  edits: EditInfo[];
  /** Incrémenté quand le contenu distant ou local a pu changer (transfert, renvoi d'édition). */
  revision: Record<SftpId, number>;
  handleEvent: (sftpId: SftpId, event: SftpEvent) => void;
  addEdit: (info: Omit<EditInfo, 'status' | 'code'>) => void;
  removeEdit: (edit: EditId) => void;
  bump: (sftpId: SftpId) => void;
  clearFinished: (sftpId: SftpId) => void;
  /** Oublie tout ce qui concerne un client fermé. */
  forget: (sftpId: SftpId) => void;
  reset: () => void;
}

export function isTransferFinished(status: TransferStatus): boolean {
  return status === 'done' || status === 'failed' || status === 'cancelled';
}

function finishedStatus(result: TransferResult): TransferStatus {
  if (result.code === 'ssh.transfer_cancelled') return 'cancelled';
  if (result.code !== null || result.failed > 0) return 'failed';
  return 'done';
}

export const useSftp = create<SftpState>()((set, get) => ({
  transfers: [],
  edits: [],
  revision: {},

  handleEvent: (sftpId, event) => {
    const patch = (job: JobId, change: Partial<Transfer>) => {
      set((s) => ({
        transfers: s.transfers.map((t) =>
          t.job === job && t.sftpId === sftpId ? { ...t, ...change } : t,
        ),
      }));
    };
    const patchEdit = (edit: EditId, change: Partial<EditInfo>) => {
      set((s) => ({
        edits: s.edits.map((e) => (e.edit === edit ? { ...e, ...change } : e)),
      }));
    };
    switch (event.type) {
      case 'transferQueued': {
        const transfer: Transfer = {
          job: event.job,
          sftpId,
          direction: event.direction,
          label: event.label,
          count: event.count,
          status: 'queued',
          bytesDone: 0,
          bytesTotal: 0,
          filesDone: 0,
          filesTotal: 0,
          conflictName: null,
          result: null,
        };
        set((s) => {
          const finished = s.transfers.filter((t) => isTransferFinished(t.status));
          const excess = finished.length - MAX_FINISHED + 1;
          const dropped = new Set(excess > 0 ? finished.slice(0, excess).map((t) => t.job) : []);
          return { transfers: [...s.transfers.filter((t) => !dropped.has(t.job)), transfer] };
        });
        break;
      }
      case 'transferStarted':
        patch(event.job, { status: 'running' });
        break;
      case 'transferProgress':
        patch(event.job, {
          status: 'running',
          conflictName: null,
          bytesDone: event.bytesDone,
          bytesTotal: event.bytesTotal,
          filesDone: event.filesDone,
          filesTotal: event.filesTotal,
        });
        break;
      case 'transferConflict':
        patch(event.job, { status: 'conflict', conflictName: event.name });
        break;
      case 'transferFinished': {
        const result: TransferResult = {
          transferred: event.transferred,
          skipped: event.skipped,
          ignored: event.ignored,
          refused: event.refused,
          failed: event.failed,
          code: event.code,
        };
        patch(event.job, { status: finishedStatus(result), conflictName: null, result });
        get().bump(sftpId);
        break;
      }
      case 'editUploaded':
        patchEdit(event.edit, { status: 'uploaded', code: null });
        get().bump(sftpId);
        break;
      case 'editConflict':
        patchEdit(event.edit, { status: 'conflict', code: 'ssh.edit_conflict' });
        break;
      case 'editFailed':
        patchEdit(event.edit, { status: 'failed', code: event.code });
        break;
    }
  },

  addEdit: (info) => {
    set((s) => ({ edits: [...s.edits, { ...info, status: 'watching', code: null }] }));
  },

  removeEdit: (edit) => {
    set((s) => ({ edits: s.edits.filter((e) => e.edit !== edit) }));
  },

  bump: (sftpId) => {
    set((s) => ({ revision: { ...s.revision, [sftpId]: (s.revision[sftpId] ?? 0) + 1 } }));
  },

  clearFinished: (sftpId) => {
    set((s) => ({
      transfers: s.transfers.filter((t) => t.sftpId !== sftpId || !isTransferFinished(t.status)),
    }));
  },

  forget: (sftpId) => {
    set((s) => {
      const revision = { ...s.revision };
      // eslint-disable-next-line @typescript-eslint/no-dynamic-delete
      delete revision[sftpId];
      return {
        transfers: s.transfers.filter((t) => t.sftpId !== sftpId),
        edits: s.edits.filter((e) => e.sftpId !== sftpId),
        revision,
      };
    });
  },

  reset: () => {
    set({ transfers: [], edits: [], revision: {} });
  },
}));
