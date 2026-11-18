import { ArrowDownToLine, ArrowUpFromLine, FilePen, Upload, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useShallow } from 'zustand/react/shallow';

import { Button } from '@/components/ui/button';
import { sftpEditClose, sftpEditForce, sftpTransferCancel } from '@/lib/ipc';
import type { SftpId } from '@/lib/types';
import { cn } from '@/lib/utils';
import { type EditInfo, isTransferFinished, type Transfer, useSftp } from '@/stores/sftp';

import { formatSize } from './format';

function TransferRow({ transfer }: { transfer: Transfer }) {
  const { t } = useTranslation();
  const finished = isTransferFinished(transfer.status);
  const Icon = transfer.direction === 'download' ? ArrowDownToLine : ArrowUpFromLine;
  const label =
    transfer.count > 1
      ? t('sftp.labelMore', { name: transfer.label, count: transfer.count - 1 })
      : transfer.label;
  const result = transfer.result;
  const problems = result ? result.refused + result.failed : 0;

  return (
    <li className="flex flex-col gap-1 border-b px-3 py-2 text-xs last:border-b-0">
      <div className="flex items-center gap-2">
        <Icon aria-hidden="true" className="size-4 shrink-0" />
        <span className="min-w-0 flex-1 truncate font-medium" title={label}>
          {label}
        </span>
        <span
          className={cn(
            'shrink-0',
            transfer.status === 'failed' && 'text-destructive',
            transfer.status === 'done' && 'text-green-700 dark:text-green-400',
          )}
        >
          {t(`sftp.status.${transfer.status}`)}
        </span>
        {!finished && (
          <Button
            size="icon"
            variant="ghost"
            className="size-6"
            aria-label={t('sftp.cancelTransfer')}
            title={t('sftp.cancelTransfer')}
            onClick={() => {
              void sftpTransferCancel(transfer.sftpId, transfer.job).catch(() => undefined);
            }}
          >
            <X aria-hidden="true" />
          </Button>
        )}
      </div>
      {!finished && (
        <>
          <progress
            className="h-1.5 w-full overflow-hidden rounded bg-muted [&::-moz-progress-bar]:bg-primary [&::-webkit-progress-bar]:bg-muted [&::-webkit-progress-value]:bg-primary"
            max={Math.max(transfer.bytesTotal, 1)}
            value={transfer.bytesDone}
            aria-label={t('sftp.progressLabel', { name: label })}
          />
          <span className="text-muted-foreground">
            {t('sftp.progress', {
              done: formatSize(transfer.bytesDone),
              total: formatSize(transfer.bytesTotal),
              files: transfer.filesDone,
              filesTotal: transfer.filesTotal,
            })}
          </span>
        </>
      )}
      {finished && result && (
        <span className={cn('text-muted-foreground', problems > 0 && 'text-destructive')}>
          {t('sftp.summary', { count: result.transferred })}
          {result.skipped > 0 && ` · ${t('sftp.summarySkipped', { count: result.skipped })}`}
          {result.ignored > 0 && ` · ${t('sftp.summaryIgnored', { count: result.ignored })}`}
          {result.refused > 0 && ` · ${t('sftp.summaryRefused', { count: result.refused })}`}
          {result.failed > 0 && ` · ${t('sftp.summaryFailed', { count: result.failed })}`}
          {result.code !== null &&
            result.code !== 'ssh.transfer_cancelled' &&
            ` · ${t(`errors.${result.code}`, { defaultValue: t('errors.unknown') })}`}
        </span>
      )}
    </li>
  );
}

function EditRow({ edit }: { edit: EditInfo }) {
  const { t } = useTranslation();
  return (
    <li className="flex flex-col gap-1 border-b px-3 py-2 text-xs last:border-b-0">
      <div className="flex items-center gap-2">
        <FilePen aria-hidden="true" className="size-4 shrink-0" />
        <span className="min-w-0 flex-1 truncate font-medium" title={edit.label}>
          {edit.label}
        </span>
        <span
          className={cn(
            'shrink-0',
            edit.status !== 'watching' && edit.status !== 'uploaded' && 'text-destructive',
          )}
        >
          {t(`sftp.editStatus.${edit.status}`)}
        </span>
        {edit.status === 'conflict' && (
          <Button
            size="icon"
            variant="ghost"
            className="size-6"
            aria-label={t('sftp.editForce')}
            title={t('sftp.editForce')}
            onClick={() => {
              void sftpEditForce(edit.edit).catch(() => undefined);
            }}
          >
            <Upload aria-hidden="true" />
          </Button>
        )}
        <Button
          size="icon"
          variant="ghost"
          className="size-6"
          aria-label={t('sftp.editStop')}
          title={t('sftp.editStop')}
          onClick={() => {
            void sftpEditClose(edit.edit).catch(() => undefined);
            useSftp.getState().removeEdit(edit.edit);
          }}
        >
          <X aria-hidden="true" />
        </Button>
      </div>
      {edit.code !== null && (
        <span className="text-destructive">
          {t(`errors.${edit.code}`, { defaultValue: t('errors.unknown') })}
        </span>
      )}
    </li>
  );
}

/** Transferts et éditions d'un client SFTP. */
export function TransferList({ sftpId, className }: { sftpId: SftpId; className?: string }) {
  const { t } = useTranslation();
  const transfers = useSftp(useShallow((s) => s.transfers.filter((x) => x.sftpId === sftpId)));
  const edits = useSftp(useShallow((s) => s.edits.filter((x) => x.sftpId === sftpId)));
  if (transfers.length === 0 && edits.length === 0) return null;
  const anyFinished = transfers.some((x) => isTransferFinished(x.status));

  return (
    <section
      aria-label={t('sftp.transfers')}
      className={cn('flex max-h-56 flex-col border-t', className)}
    >
      <div className="flex items-center justify-between px-3 py-1">
        <h3 className="text-xs font-semibold uppercase text-muted-foreground">
          {t('sftp.transfers')}
        </h3>
        {anyFinished && (
          <Button
            size="sm"
            variant="ghost"
            className="h-6 text-xs"
            onClick={() => {
              useSftp.getState().clearFinished(sftpId);
            }}
          >
            {t('sftp.clearFinished')}
          </Button>
        )}
      </div>
      <ul className="overflow-auto">
        {edits.map((edit) => (
          <EditRow key={`e${String(edit.edit)}`} edit={edit} />
        ))}
        {[...transfers].reverse().map((transfer) => (
          <TransferRow key={transfer.job} transfer={transfer} />
        ))}
      </ul>
    </section>
  );
}
