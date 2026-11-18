import { ArrowUp, File, Folder, FolderOpen, Link, LoaderCircle, RotateCw } from 'lucide-react';
import { type DragEvent, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import { localList, localPickRoot, sftpDownload, toAppError } from '@/lib/ipc';
import type { LocalEntry, LocalListing, LocalRootRef, SftpId } from '@/lib/types';
import { cn } from '@/lib/utils';
import { useSftp } from '@/stores/sftp';

import { LOCAL_DRAG_TYPE, REMOTE_DRAG_TYPE, readRemoteDrag } from './dnd';
import { formatSize, joinLocal, parentLocal } from './format';

interface LocalBrowserProps {
  sftpId: SftpId;
  root: LocalRootRef | null;
  path: string;
  onRootChange: (root: LocalRootRef) => void;
  onPathChange: (path: string) => void;
}

/**
 * Panneau local du double panneau (SFTP-02). Le dossier est choisi dans la boîte de dialogue
 * du système, ouverte par le cœur : l'interface ne voit que son nom et des chemins relatifs,
 * et ne peut ni supprimer ni renommer de fichier local (ADR-034).
 */
export function LocalBrowser({
  sftpId,
  root,
  path,
  onRootChange,
  onPathChange,
}: LocalBrowserProps) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<{
    key: string;
    listing: LocalListing | null;
    errorCode: string | null;
  } | null>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);
  const [selection, setSelection] = useState<{ key: string; names: Set<string> } | null>(null);
  const [dropActive, setDropActive] = useState(false);
  const revision = useSftp((s) => s.revision[sftpId] ?? 0);

  // Rechargé à chaque changement de dossier, après un transfert terminé (révision) ou sur
  // demande.
  const place = `${String(root?.id ?? '')}|${path}`;
  const key = `${place}|${String(revision)}|${String(reloadToken)}`;
  useEffect(() => {
    if (!root) return;
    let cancelled = false;
    localList(root.id, path)
      .then((listing) => {
        if (!cancelled) setLoaded({ key, listing, errorCode: null });
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoaded({ key, listing: null, errorCode: toAppError(error).code });
      });
    return () => {
      cancelled = true;
    };
  }, [root, path, key]);

  const loading = root !== null && loaded?.key !== key;
  const selected = selection?.key === place ? selection.names : new Set<string>();
  const errorCode = actionError ?? (loaded?.key === key ? loaded.errorCode : null);

  async function pickRoot() {
    try {
      const picked = await localPickRoot();
      if (picked) onRootChange(picked);
    } catch (error) {
      setActionError(toAppError(error).code);
    }
  }

  function onDragStart(event: DragEvent<HTMLLIElement>, entry: LocalEntry) {
    if (!root) return;
    const names = selected.has(entry.name) ? [...selected] : [entry.name];
    event.dataTransfer.setData(
      LOCAL_DRAG_TYPE,
      JSON.stringify({ rootId: root.id, paths: names.map((name) => joinLocal(path, name)) }),
    );
    event.dataTransfer.effectAllowed = 'copy';
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    setDropActive(false);
    const remote = readRemoteDrag(event.dataTransfer);
    if (!remote || !root || remote.sftpId !== sftpId) return;
    event.preventDefault();
    setActionError(null);
    sftpDownload(sftpId, remote.paths, { kind: 'root', rootId: root.id, path }).catch(
      (error: unknown) => {
        setActionError(toAppError(error).code);
      },
    );
  }

  const listing = loaded?.key === key ? loaded.listing : (loaded?.listing ?? null);
  const entries = listing?.entries ?? [];

  return (
    <div
      className={cn(
        'relative flex min-h-0 flex-1 flex-col',
        dropActive && 'ring-2 ring-primary ring-inset',
      )}
      onDragOver={(event) => {
        if (root && event.dataTransfer.types.includes(REMOTE_DRAG_TYPE)) {
          event.preventDefault();
          event.dataTransfer.dropEffect = 'copy';
          setDropActive(true);
        }
      }}
      onDragLeave={() => {
        setDropActive(false);
      }}
      onDrop={onDrop}
    >
      <div
        role="toolbar"
        aria-label={t('sftp.localToolbar')}
        className="flex items-center gap-0.5 border-b px-1 py-1"
      >
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          aria-label={t('sftp.up')}
          title={t('sftp.up')}
          disabled={!root || path === ''}
          onClick={() => {
            onPathChange(parentLocal(path));
          }}
        >
          <ArrowUp aria-hidden="true" />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          aria-label={t('common.refresh')}
          title={t('common.refresh')}
          disabled={!root}
          onClick={() => {
            setActionError(null);
            setReloadToken((n) => n + 1);
          }}
        >
          <RotateCw aria-hidden="true" />
        </Button>
        <Button size="sm" variant="ghost" onClick={() => void pickRoot()}>
          <FolderOpen aria-hidden="true" />
          {t('sftp.pickLocal')}
        </Button>
        {loading && <LoaderCircle aria-hidden="true" className="ml-1 size-4 animate-spin" />}
      </div>
      <p
        className="truncate border-b px-2 py-1.5 font-mono text-xs text-muted-foreground"
        title={path}
      >
        {root ? `${root.name}${path === '' ? '' : `/${path}`}` : t('sftp.noLocal')}
      </p>
      {errorCode && (
        <div className="px-2 pt-2">
          <ErrorMessage code={errorCode} />
        </div>
      )}
      {!root && (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-sm text-muted-foreground">
          <p>{t('sftp.localHint')}</p>
          <Button variant="outline" onClick={() => void pickRoot()}>
            <FolderOpen aria-hidden="true" />
            {t('sftp.pickLocal')}
          </Button>
        </div>
      )}
      {root && (
        <ul
          role="listbox"
          aria-label={t('sftp.localFiles')}
          aria-multiselectable="true"
          className="min-h-0 flex-1 overflow-auto py-1 text-sm"
        >
          {listing?.truncated && (
            <li className="px-3 text-xs text-muted-foreground">{t('sftp.truncated')}</li>
          )}
          {entries.length === 0 && !loading && (
            <li className="px-3 py-2 text-xs text-muted-foreground">{t('sftp.empty')}</li>
          )}
          {entries.map((entry) => {
            const Icon = entry.kind === 'symlink' ? Link : entry.kind === 'dir' ? Folder : File;
            const isSelected = selected.has(entry.name);
            return (
              <li
                key={entry.name}
                role="option"
                aria-selected={isSelected}
                tabIndex={0}
                draggable
                onDragStart={(event) => {
                  onDragStart(event, entry);
                }}
                onClick={(event) => {
                  if (!event.ctrlKey && !event.metaKey) {
                    setSelection({ key: place, names: new Set([entry.name]) });
                    return;
                  }
                  const next = new Set(selected);
                  if (next.has(entry.name)) next.delete(entry.name);
                  else next.add(entry.name);
                  setSelection({ key: place, names: next });
                }}
                onDoubleClick={() => {
                  if (entry.kind === 'dir' || entry.kind === 'symlink')
                    onPathChange(joinLocal(path, entry.name));
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && (entry.kind === 'dir' || entry.kind === 'symlink')) {
                    onPathChange(joinLocal(path, entry.name));
                  }
                }}
                className={cn(
                  'flex cursor-default items-center gap-2 px-2 py-0.5 outline-none select-none',
                  'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset',
                  isSelected ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/50',
                )}
              >
                <Icon
                  aria-hidden="true"
                  className={cn('size-4 shrink-0', entry.kind === 'dir' && 'text-primary')}
                />
                <span className="min-w-0 flex-1 truncate" title={entry.name}>
                  {entry.name}
                </span>
                <span className="w-16 shrink-0 text-right text-xs text-muted-foreground">
                  {entry.kind === 'file' ? formatSize(entry.size) : ''}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
