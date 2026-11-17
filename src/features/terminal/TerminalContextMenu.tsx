import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';

import { formatShortcut } from '@/features/shortcuts/shortcuts';
import { cn } from '@/lib/utils';

interface TerminalContextMenuProps {
  x: number;
  y: number;
  canCopy: boolean;
  onCopy: () => void;
  onPaste: () => void;
  onSearch: () => void;
  onClose: () => void;
}

/** Menu du clic droit dans le terminal (SSH-05). Navigable au clavier, fermé par Échap. */
export function TerminalContextMenu({
  x,
  y,
  canCopy,
  onCopy,
  onPaste,
  onSearch,
  onClose,
}: TerminalContextMenuProps) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>('button:not([disabled])')?.focus();
    const close = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    window.addEventListener('mousedown', close);
    return () => {
      window.removeEventListener('mousedown', close);
    };
  }, [onClose]);

  const items = [
    { label: t('terminal.copy'), action: onCopy, disabled: !canCopy, shortcut: 'copy' as const },
    { label: t('terminal.paste'), action: onPaste, disabled: false, shortcut: 'paste' as const },
    { label: t('terminal.search'), action: onSearch, disabled: false, shortcut: 'search' as const },
  ];

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t('terminal.menu')}
      // Position dynamique : seule exception admise au style en ligne (règles frontend).
      style={{ left: x, top: y }}
      className="fixed z-50 min-w-48 rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
      onKeyDown={(e) => {
        const buttons = [
          ...(ref.current?.querySelectorAll<HTMLButtonElement>('button:not([disabled])') ?? []),
        ];
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        if (e.key === 'Escape') {
          e.preventDefault();
          onClose();
        } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          const step = e.key === 'ArrowDown' ? 1 : -1;
          buttons[(index + step + buttons.length) % buttons.length]?.focus();
        }
      }}
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          role="menuitem"
          disabled={item.disabled}
          onClick={() => {
            onClose();
            item.action();
          }}
          className={cn(
            'flex w-full items-center justify-between gap-6 rounded-sm px-2 py-1.5 text-sm outline-none',
            'hover:bg-accent focus-visible:bg-accent disabled:pointer-events-none disabled:opacity-50',
          )}
        >
          <span>{item.label}</span>
          <span className="text-xs text-muted-foreground">{formatShortcut(item.shortcut, t)}</span>
        </button>
      ))}
    </div>
  );
}
