import {
  ArrowUp,
  CircleAlert,
  Download,
  File,
  FilePen,
  Folder,
  FolderPlus,
  FolderUp,
  House,
  KeyRound,
  Link,
  LoaderCircle,
  Pencil,
  RotateCw,
  Trash2,
  Upload,
} from 'lucide-react';
import { type DragEvent, type KeyboardEvent, useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { ConfirmDialog } from '@/components/ConfirmDialog';
import { NameDialog } from '@/components/NameDialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ErrorMessage } from '@/features/connect/ErrorMessage';
import {
  sftpChmod,
  sftpDownload,
  sftpEdit,
  sftpHome,
  sftpList,
  sftpMkdir,
  sftpRemove,
  sftpRename,
  sftpUpload,
  toAppError,
} from '@/lib/ipc';
import type { DownloadTarget, RemoteEntry, RemoteListing, SftpId } from '@/lib/types';
import { cn } from '@/lib/utils';
import { useSftp } from '@/stores/sftp';

import { ChmodDialog } from './ChmodDialog';
import { LOCAL_DRAG_TYPE, REMOTE_DRAG_TYPE, readLocalDrag } from './dnd';
import {
  formatDate,
  formatPermissions,
  formatSize,
  isDirLike,
  joinRemote,
  parentRemote,
} from './format';

/** Longueur maximale d'un nom de fichier (comme le cœur, en octets ; approximé en caractères). */
const MAX_FILE_NAME = 255;

/** Noms affichés dans une confirmation : les 5 premiers, puis « … ». */
function previewNames(entries: RemoteEntry[]): string {
  const names = entries.slice(0, 5).map((e) => e.displayName);
  return entries.length > 5 ? `${names.join(', ')}, …` : names.join(', ');
}

interface Loaded {
  /** Chargement auquel ce résultat correspond (dossier, révision, actualisation). */
  key: string;
  listing: RemoteListing | null;
  errorCode: string | null;
}

interface Selection {
  path: string;
  names: Set<string>;
  focused: number;
}

const EMPTY_SELECTION: Selection = { path: '', names: new Set(), focused: 0 };

type Dialog =
  | { kind: 'mkdir' }
  | { kind: 'rename'; entry: RemoteEntry }
  | { kind: 'delete'; entries: RemoteEntry[] }
  | { kind: 'chmod'; entry: RemoteEntry };

interface RemoteBrowserProps {
  sftpId: SftpId;
  /** Dossier affiché ; `null` : dossier personnel au premier affichage. */
  path: string | null;
  onPathChange: (path: string) => void;
  /** Affichage étroit (panneau latéral) : colonnes réduites. */
  compact?: boolean;
  /** Dossier local courant (double panneau) : destination des téléchargements. */
  localTarget?: { rootId: number; path: string } | null;
}

/** Explorateur de fichiers distant (SFTP-03). */
export function RemoteBrowser({
  sftpId,
  path,
  onPathChange,
  compact = false,
  localTarget = null,
}: RemoteBrowserProps) {
  const { t, i18n } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const [actionError, setActionError] = useState<{ path: string; code: string } | null>(null);
  const [selectionState, setSelectionState] = useState<Selection>(EMPTY_SELECTION);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [draft, setDraft] = useState<{ forPath: string | null; value: string } | null>(null);
  const [dropActive, setDropActive] = useState(false);
  const revision = useSftp((s) => s.revision[sftpId] ?? 0);

  // Dossier personnel au premier affichage.
  useEffect(() => {
    if (path !== null) return;
    let cancelled = false;
    sftpHome(sftpId)
      .then((home) => {
        if (!cancelled) onPathChange(home);
      })
      .catch(() => {
        if (!cancelled) onPathChange('/');
      });
    return () => {
      cancelled = true;
    };
  }, [sftpId, path, onPathChange]);

  // Chargement du dossier : à chaque changement de dossier, après un transfert terminé ou une
  // édition renvoyée (révision), ou sur demande (actualiser, après une opération).
  const key = `${path ?? ''}|${String(revision)}|${String(reloadToken)}`;
  useEffect(() => {
    if (path === null) return;
    let cancelled = false;
    sftpList(sftpId, path)
      .then((listing) => {
        if (!cancelled) setLoaded({ key, listing, errorCode: null });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setLoaded((previous) => ({
            key,
            listing: previous?.listing ?? null,
            errorCode: toAppError(error).code,
          }));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [sftpId, path, key]);

  const reload = useCallback(() => {
    setReloadToken((n) => n + 1);
  }, []);

  const loading = path !== null && loaded?.key !== key;
  const listing = loaded?.listing ?? null;
  const current = listing?.path ?? path ?? '/';
  const entries = listing?.entries ?? [];
  // La sélection et le focus valent pour le dossier affiché, et se vident quand il change.
  const ownSelection = selectionState.path === current ? selectionState : EMPTY_SELECTION;
  const selected = ownSelection.names;
  const focused = Math.min(ownSelection.focused, Math.max(entries.length - 1, 0));
  const selection = entries.filter((e) => selected.has(e.name));
  const single = selection.length === 1 ? selection[0] : undefined;
  const loadError = loaded?.key === key ? loaded.errorCode : null;
  const errorCode = actionError?.path === current ? actionError.code : loadError;
  const address = draft !== null && draft.forPath === path ? draft.value : (path ?? '');

  function setSelected(names: Set<string>, focus = focused) {
    setSelectionState({ path: current, names, focused: focus });
  }

  function open(entry: RemoteEntry) {
    if (isDirLike(entry.kind, entry.linkTarget)) {
      onPathChange(joinRemote(current, entry.name));
    } else if (entry.kind === 'file' || entry.linkTarget === 'file') {
      void edit(entry);
    }
  }

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
      setActionError(null);
    } catch (error) {
      setActionError({ path: current, code: toAppError(error).code });
    }
  }

  function download(items: RemoteEntry[]) {
    if (items.length === 0) return;
    const target: DownloadTarget = localTarget
      ? { kind: 'root', rootId: localTarget.rootId, path: localTarget.path }
      : { kind: 'pick' };
    void run(() =>
      sftpDownload(
        sftpId,
        items.map((e) => joinRemote(current, e.name)),
        target,
      ),
    );
  }

  async function edit(entry: RemoteEntry) {
    await run(async () => {
      const id = await sftpEdit(sftpId, joinRemote(current, entry.name), (event) => {
        useSftp.getState().handleEvent(sftpId, event);
      });
      useSftp.getState().addEdit({ edit: id, sftpId, label: entry.displayName });
    });
  }

  function select(index: number, event: { ctrlKey: boolean; metaKey: boolean }) {
    const entry = entries[index];
    if (!entry) return;
    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selected);
      if (next.has(entry.name)) next.delete(entry.name);
      else next.add(entry.name);
      setSelected(next, index);
    } else {
      setSelected(new Set([entry.name]), index);
    }
  }

  function onListKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    const entry = entries[focused];
    switch (event.key) {
      case 'ArrowDown':
      case 'ArrowUp': {
        event.preventDefault();
        const next = Math.min(
          Math.max(focused + (event.key === 'ArrowDown' ? 1 : -1), 0),
          entries.length - 1,
        );
        const name = entries[next]?.name;
        if (!event.ctrlKey && name !== undefined) setSelected(new Set([name]), next);
        else setSelected(selected, next);
        document.getElementById(`${String(sftpId)}-row-${String(next)}`)?.focus();
        break;
      }
      case ' ':
        event.preventDefault();
        select(focused, { ctrlKey: true, metaKey: false });
        break;
      case 'Enter':
        event.preventDefault();
        if (entry) open(entry);
        break;
      case 'Backspace':
        event.preventDefault();
        onPathChange(parentRemote(current));
        break;
      case 'Delete':
        if (selection.length > 0) setDialog({ kind: 'delete', entries: selection });
        break;
      case 'F2':
        event.preventDefault();
        if (single) setDialog({ kind: 'rename', entry: single });
        break;
    }
  }

  function onDragStart(event: DragEvent<HTMLLIElement>, entry: RemoteEntry) {
    const items = selected.has(entry.name) ? selection : [entry];
    event.dataTransfer.setData(
      REMOTE_DRAG_TYPE,
      JSON.stringify({ sftpId, paths: items.map((e) => joinRemote(current, e.name)) }),
    );
    event.dataTransfer.effectAllowed = 'copy';
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    setDropActive(false);
    const local = readLocalDrag(event.dataTransfer);
    if (!local) return;
    event.preventDefault();
    void run(() =>
      sftpUpload(sftpId, { kind: 'root', rootId: local.rootId, paths: local.paths }, current),
    );
  }

  const toolbarButton = (
    label: string,
    icon: React.ReactNode,
    onClick: () => void,
    disabled = false,
  ) => (
    <Button
      size="icon"
      variant="ghost"
      className="size-8"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      {icon}
    </Button>
  );

  return (
    <div
      className={cn(
        'relative flex min-h-0 flex-1 flex-col',
        dropActive && 'ring-2 ring-primary ring-inset',
      )}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes(LOCAL_DRAG_TYPE)) {
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
        aria-label={t('sftp.remoteToolbar')}
        className="flex flex-wrap items-center gap-0.5 border-b px-1 py-1"
      >
        {toolbarButton(
          t('sftp.up'),
          <ArrowUp aria-hidden="true" />,
          () => {
            onPathChange(parentRemote(current));
          },
          current === '/',
        )}
        {toolbarButton(t('sftp.home'), <House aria-hidden="true" />, () => {
          void sftpHome(sftpId)
            .then(onPathChange)
            .catch(() => undefined);
        })}
        {toolbarButton(t('common.refresh'), <RotateCw aria-hidden="true" />, () => {
          reload();
        })}
        <span className="mx-1 h-5 w-px bg-border" aria-hidden="true" />
        {toolbarButton(t('sftp.uploadFiles'), <Upload aria-hidden="true" />, () => {
          void run(() => sftpUpload(sftpId, { kind: 'pickFiles' }, current));
        })}
        {toolbarButton(t('sftp.uploadFolder'), <FolderUp aria-hidden="true" />, () => {
          void run(() => sftpUpload(sftpId, { kind: 'pickFolder' }, current));
        })}
        {toolbarButton(
          localTarget ? t('sftp.downloadHere') : t('sftp.download'),
          <Download aria-hidden="true" />,
          () => {
            download(selection);
          },
          selection.length === 0,
        )}
        {toolbarButton(t('sftp.newFolder'), <FolderPlus aria-hidden="true" />, () => {
          setDialog({ kind: 'mkdir' });
        })}
        {toolbarButton(
          t('sftp.edit'),
          <FilePen aria-hidden="true" />,
          () => {
            if (single) void edit(single);
          },
          !single || isDirLike(single.kind, single.linkTarget),
        )}
        {toolbarButton(
          t('common.rename'),
          <Pencil aria-hidden="true" />,
          () => {
            if (single) setDialog({ kind: 'rename', entry: single });
          },
          !single,
        )}
        {toolbarButton(
          t('sftp.chmod'),
          <KeyRound aria-hidden="true" />,
          () => {
            if (single) setDialog({ kind: 'chmod', entry: single });
          },
          !single,
        )}
        {toolbarButton(
          t('common.delete'),
          <Trash2 aria-hidden="true" />,
          () => {
            setDialog({ kind: 'delete', entries: selection });
          },
          selection.length === 0,
        )}
        {loading && <LoaderCircle aria-hidden="true" className="ml-1 size-4 animate-spin" />}
      </div>

      <form
        className="border-b px-2 py-1"
        onSubmit={(event) => {
          event.preventDefault();
          const value = address.trim();
          if (value.startsWith('/')) onPathChange(value);
          else setActionError({ path: current, code: 'ssh.sftp_invalid_path' });
        }}
      >
        <Input
          aria-label={t('sftp.remotePath')}
          value={address}
          onChange={(e) => {
            setDraft({ forPath: path, value: e.target.value });
          }}
          maxLength={4096}
          spellCheck={false}
          autoComplete="off"
          className="h-7 font-mono text-xs"
        />
      </form>

      {errorCode && (
        <div className="px-2 pt-2">
          <ErrorMessage code={errorCode} />
        </div>
      )}
      {listing?.truncated && (
        <p className="px-2 pt-2 text-xs text-muted-foreground">{t('sftp.truncated')}</p>
      )}

      <ul
        role="listbox"
        aria-label={t('sftp.remoteFiles')}
        aria-multiselectable="true"
        className="min-h-0 flex-1 overflow-auto py-1 text-sm"
        onKeyDown={onListKeyDown}
      >
        {entries.length === 0 && !loading && !errorCode && (
          <li className="px-3 py-2 text-xs text-muted-foreground">{t('sftp.empty')}</li>
        )}
        {entries.map((entry, index) => {
          const dir = isDirLike(entry.kind, entry.linkTarget);
          const Icon = entry.kind === 'symlink' ? Link : dir ? Folder : File;
          return (
            <li
              key={entry.name}
              id={`${String(sftpId)}-row-${String(index)}`}
              role="option"
              aria-selected={selected.has(entry.name)}
              tabIndex={index === focused ? 0 : -1}
              draggable
              onDragStart={(event) => {
                onDragStart(event, entry);
              }}
              onClick={(event) => {
                select(index, event);
              }}
              onDoubleClick={() => {
                open(entry);
              }}
              className={cn(
                'flex cursor-default items-center gap-2 px-2 py-0.5 outline-none select-none',
                'focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset',
                selected.has(entry.name)
                  ? 'bg-accent text-accent-foreground'
                  : 'hover:bg-accent/50',
              )}
            >
              <Icon aria-hidden="true" className={cn('size-4 shrink-0', dir && 'text-primary')} />
              <span className="min-w-0 flex-1 truncate" title={entry.displayName}>
                {entry.displayName}
              </span>
              {entry.suspicious && (
                <span
                  title={t('sftp.suspicious')}
                  className="shrink-0 text-amber-600 dark:text-amber-400"
                >
                  <CircleAlert aria-hidden="true" className="size-4" />
                  <span className="sr-only">{t('sftp.suspicious')}</span>
                </span>
              )}
              {!compact && (
                <span className="w-20 shrink-0 text-right font-mono text-xs text-muted-foreground">
                  {entry.permissions !== null ? formatPermissions(entry.permissions) : ''}
                </span>
              )}
              {!compact && (
                <span className="hidden w-32 shrink-0 text-right text-xs text-muted-foreground lg:inline">
                  {formatDate(entry.modified, i18n.language)}
                </span>
              )}
              <span className="w-16 shrink-0 text-right text-xs text-muted-foreground">
                {entry.kind === 'file' ? formatSize(entry.size) : ''}
              </span>
            </li>
          );
        })}
      </ul>

      {dialog?.kind === 'mkdir' && (
        <NameDialog
          title={t('sftp.newFolder')}
          label={t('sftp.name')}
          initial=""
          maxLength={MAX_FILE_NAME}
          onSave={async (name) => {
            await sftpMkdir(sftpId, joinRemote(current, name));
            reload();
          }}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'rename' && (
        <NameDialog
          title={t('common.rename')}
          label={t('sftp.name')}
          initial={dialog.entry.name}
          maxLength={MAX_FILE_NAME}
          onSave={async (name) => {
            await sftpRename(
              sftpId,
              joinRemote(current, dialog.entry.name),
              joinRemote(current, name),
            );
            reload();
          }}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'chmod' && (
        <ChmodDialog
          name={dialog.entry.displayName}
          initial={dialog.entry.permissions}
          onSave={async (mode) => {
            await sftpChmod(sftpId, joinRemote(current, dialog.entry.name), mode);
            reload();
          }}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
      {dialog?.kind === 'delete' && (
        <ConfirmDialog
          title={t('sftp.deleteTitle', { count: dialog.entries.length })}
          message={
            dialog.entries.some((e) => e.kind === 'dir')
              ? t('sftp.deleteRecursive', {
                  names: previewNames(dialog.entries),
                })
              : t('sftp.deleteMessage', {
                  names: previewNames(dialog.entries),
                })
          }
          confirmLabel={t('common.delete')}
          destructive
          onCancel={() => {
            setDialog(null);
          }}
          onConfirm={() => {
            const items = dialog.entries;
            setDialog(null);
            void run(async () => {
              for (const item of items) await sftpRemove(sftpId, joinRemote(current, item.name));
            }).then(reload);
          }}
        />
      )}
    </div>
  );
}
