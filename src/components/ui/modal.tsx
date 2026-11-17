import { type ReactNode, useEffect, useRef } from 'react';

import { cn } from '@/lib/utils';

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface ModalProps {
  /** Identifiant du titre (aria-labelledby). */
  labelledBy: string;
  /** Identifiant de la description (aria-describedby). */
  describedBy?: string;
  /** Fermeture par Échap ; absent : la fenêtre ne se ferme que par ses boutons. */
  onClose?: () => void;
  /** `alertdialog` pour une alerte qui exige une réponse. */
  role?: 'dialog' | 'alertdialog';
  /** `panel` : limitée au panneau parent (positionné), pour ne pas bloquer les autres onglets. */
  scope?: 'panel' | 'window';
  className?: string;
  children: ReactNode;
}

/**
 * Fenêtre modale accessible, sans dépendance : focus placé sur l'élément marqué
 * `data-autofocus` (sinon le premier élément focalisable), tabulation piégée, Échap,
 * et focus rendu à l'élément précédent à la fermeture. N'injecte aucune balise <style>.
 */
export function Modal({
  labelledBy,
  describedBy,
  onClose,
  role = 'dialog',
  scope = 'panel',
  className,
  children,
}: ModalProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const node = ref.current;
    const target =
      node?.querySelector<HTMLElement>('[data-autofocus]') ??
      node?.querySelector<HTMLElement>(FOCUSABLE);
    (target ?? node)?.focus();
    return () => {
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'Escape' && onClose) {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key !== 'Tab' || !ref.current) return;
    const items = [...ref.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return (
    <div
      className={cn(
        'z-40 flex items-center justify-center bg-black/50 p-4',
        scope === 'panel' ? 'absolute inset-0' : 'fixed inset-0',
      )}
    >
      <div
        ref={ref}
        role={role}
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className={cn(
          'flex max-h-full w-full max-w-lg flex-col gap-4 overflow-auto rounded-lg border bg-background p-6 text-foreground shadow-lg outline-none',
          className,
        )}
      >
        {children}
      </div>
    </div>
  );
}
