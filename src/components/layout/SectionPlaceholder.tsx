import { useTranslation } from 'react-i18next';

import type { Section } from '@/stores/ui';

type PlaceholderSection = Exclude<Section, 'settings' | 'hosts' | 'keys'>;

/** Étape de la feuille de route où chaque section devient fonctionnelle. */
const STEP_BY_SECTION: Record<PlaceholderSection, number> = {
  snippets: 5,
  tunnels: 6,
  sftp: 4,
};

/** Contenu provisoire d'une section pas encore implémentée. */
export function SectionPlaceholder({ section }: { section: PlaceholderSection }) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
      <h1 className="text-xl font-semibold">{t(`nav.${section}`)}</h1>
      <p className="text-sm text-muted-foreground">{t(`sections.${section}`)}</p>
      <p className="text-xs text-muted-foreground">
        {t('sections.comingSoon', { step: STEP_BY_SECTION[section] })}
      </p>
    </div>
  );
}
