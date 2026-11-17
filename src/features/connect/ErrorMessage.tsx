import { CircleAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';

/** Message d'erreur traduit à partir d'un code (`errors.<code>`), avec repli générique. */
export function ErrorMessage({ code }: { code: string }) {
  const { t } = useTranslation();
  return (
    <p
      role="alert"
      className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
    >
      <CircleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
      <span>{t(`errors.${code}`, { defaultValue: t('errors.unknown') })}</span>
    </p>
  );
}
