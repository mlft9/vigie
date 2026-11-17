import { House } from 'lucide-react';
import { useTranslation } from 'react-i18next';

/**
 * Zone d'onglets. À l'étape 0, seul l'onglet d'accueil existe ;
 * les onglets de session (SSH, SFTP, RDP…) s'y ajouteront à partir de l'étape 1.
 */
export function TabBar() {
  const { t } = useTranslation();

  return (
    <div
      role="tablist"
      aria-label={t('tabs.label')}
      className="flex h-10 shrink-0 items-end gap-1 border-b bg-sidebar px-2"
    >
      <button
        type="button"
        role="tab"
        id="tab-home"
        aria-selected="true"
        aria-controls="panel-home"
        className="flex h-8 items-center gap-2 rounded-t-md border border-b-0 bg-background px-3 text-sm outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
      >
        <House aria-hidden="true" className="size-4" />
        <span>{t('tabs.home')}</span>
      </button>
    </div>
  );
}
