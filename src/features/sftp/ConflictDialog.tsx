import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Modal } from '@/components/ui/modal';
import { sftpConflictDecision, sftpTransferCancel } from '@/lib/ipc';
import type { ConflictAction } from '@/lib/types';
import { type Transfer, useSftp } from '@/stores/sftp';

/**
 * Question posée quand la destination d'un transfert existe déjà (SFTP-04). Le focus part sur
 * « Ignorer » : l'action par défaut n'écrase rien.
 */
export function ConflictDialog({ transfer }: { transfer: Transfer }) {
  const { t } = useTranslation();
  const id = useId();
  const [applyToAll, setApplyToAll] = useState(false);

  function decide(action: ConflictAction) {
    useSftp.getState().handleEvent(transfer.sftpId, {
      type: 'transferStarted',
      job: transfer.job,
    });
    void sftpConflictDecision(transfer.sftpId, transfer.job, action, applyToAll).catch(
      () => undefined,
    );
  }

  return (
    <Modal
      labelledBy={`${id}-title`}
      describedBy={`${id}-message`}
      role="alertdialog"
      scope="panel"
      onClose={() => {
        decide('skip');
      }}
    >
      <h2 id={`${id}-title`} className="text-lg font-semibold">
        {t('sftp.conflictTitle')}
      </h2>
      <p id={`${id}-message`} className="text-sm break-all">
        {t(transfer.direction === 'download' ? 'sftp.conflictLocal' : 'sftp.conflictRemote', {
          name: transfer.conflictName ?? '',
        })}
      </p>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={applyToAll}
          onChange={(e) => {
            setApplyToAll(e.target.checked);
          }}
          className="size-4"
        />
        {t('sftp.applyToAll')}
      </label>
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          variant="ghost"
          onClick={() => {
            void sftpTransferCancel(transfer.sftpId, transfer.job).catch(() => undefined);
          }}
        >
          {t('sftp.cancelTransfer')}
        </Button>
        <Button
          variant="outline"
          onClick={() => {
            decide('skip');
          }}
          data-autofocus
        >
          {t('sftp.skip')}
        </Button>
        <Button
          variant="outline"
          onClick={() => {
            decide('rename');
          }}
        >
          {t('sftp.keepBoth')}
        </Button>
        <Button
          variant="destructive"
          onClick={() => {
            decide('overwrite');
          }}
        >
          {t('sftp.overwrite')}
        </Button>
      </div>
    </Modal>
  );
}
