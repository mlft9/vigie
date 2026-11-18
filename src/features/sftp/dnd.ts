// Glisser-déposer entre les panneaux local et distant (SFTP-02).
//
// Les données glissées restent internes à la fenêtre (types MIME propres à Younji) : un dépôt
// venant d'ailleurs est ignoré. Elles sont revalidées ici, puis par le cœur.

/** Éléments distants glissés : `{ sftpId, paths }`. */
export const REMOTE_DRAG_TYPE = 'application/x-younji-remote';
/** Éléments locaux glissés : `{ rootId, paths }` (chemins relatifs au dossier choisi). */
export const LOCAL_DRAG_TYPE = 'application/x-younji-local';

/** Nombre maximal d'éléments glissés (comme la sélection admise par le cœur). */
const MAX_ITEMS = 1000;

export interface RemoteDrag {
  sftpId: number;
  paths: string[];
}

export interface LocalDrag {
  rootId: number;
  paths: string[];
}

function parse(data: DataTransfer, type: string): Record<string, unknown> | null {
  const raw = data.getData(type);
  if (raw === '') return null;
  try {
    const value: unknown = JSON.parse(raw);
    return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function stringList(value: unknown): string[] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_ITEMS) return null;
  return value.every((item): item is string => typeof item === 'string') ? value : null;
}

/** Éléments distants déposés, ou `null` si les données sont absentes ou mal formées. */
export function readRemoteDrag(data: DataTransfer): RemoteDrag | null {
  const value = parse(data, REMOTE_DRAG_TYPE);
  const paths = stringList(value?.paths);
  if (!value || typeof value.sftpId !== 'number' || !paths) return null;
  return { sftpId: value.sftpId, paths };
}

/** Éléments locaux déposés, ou `null` si les données sont absentes ou mal formées. */
export function readLocalDrag(data: DataTransfer): LocalDrag | null {
  const value = parse(data, LOCAL_DRAG_TYPE);
  const paths = stringList(value?.paths);
  if (!value || typeof value.rootId !== 'number' || !paths) return null;
  return { rootId: value.rootId, paths };
}

/** Vrai si le glisser-déposer transporte des données internes de Younji. */
function isInternal(data: DataTransfer | null): boolean {
  return (
    data !== null && (data.types.includes(REMOTE_DRAG_TYPE) || data.types.includes(LOCAL_DRAG_TYPE))
  );
}

/**
 * Ignore les fichiers déposés depuis le système (explorateur de fichiers, bureau). Sans cela,
 * le navigateur tenterait d'ouvrir le fichier déposé (navigation vers `file://`, refusée par
 * la garde de navigation du cœur). Seul le glisser-déposer interne entre les panneaux est pris
 * en charge (ADR-034). Renvoie la fonction qui retire la garde.
 */
export function installExternalDropGuard(target: Window = window): () => void {
  const block = (event: DragEvent) => {
    if (!isInternal(event.dataTransfer)) {
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'none';
    }
  };
  target.addEventListener('dragover', block);
  target.addEventListener('drop', block);
  return () => {
    target.removeEventListener('dragover', block);
    target.removeEventListener('drop', block);
  };
}
