import { type ReactNode, useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { NativeSelect } from '@/components/ui/native-select';
import { SHORTCUTS, shortcutKeys } from '@/features/shortcuts/shortcuts';
import { SUPPORTED_LANGUAGES } from '@/i18n/languages';
import { appInfo } from '@/lib/ipc';
import {
  type LanguagePreference,
  THEMES,
  type ThemePreference,
  useSettings,
} from '@/stores/settings';

const LANGUAGE_OPTIONS: readonly LanguagePreference[] = ['system', ...SUPPORTED_LANGUAGES];

function Field({
  id,
  label,
  description,
  children,
}: {
  id: string;
  label: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-sm font-medium">
        {label}
      </label>
      {children}
      <p id={`${id}-description`} className="text-xs text-muted-foreground">
        {description}
      </p>
    </div>
  );
}

function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    appInfo()
      .then((info) => {
        if (!cancelled) setVersion(info.version);
      })
      .catch(() => {
        // Hors de Tauri (navigateur, tests) : la version reste « indisponible ».
      });
    return () => {
      cancelled = true;
    };
  }, []);
  return version;
}

/** Paramètres d'apparence : langue (APP-02) et thème (APP-03). */
export function SettingsView() {
  const { t } = useTranslation();
  const theme = useSettings((s) => s.theme);
  const language = useSettings((s) => s.language);
  const setTheme = useSettings((s) => s.setTheme);
  const setLanguage = useSettings((s) => s.setLanguage);
  const version = useAppVersion();
  const languageId = useId();
  const themeId = useId();

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-8 p-8">
      <h1 className="text-xl font-semibold">{t('settings.title')}</h1>

      <section aria-labelledby={`${themeId}-heading`} className="flex flex-col gap-5">
        <h2 id={`${themeId}-heading`} className="text-sm font-semibold text-muted-foreground">
          {t('settings.appearance')}
        </h2>

        <Field
          id={languageId}
          label={t('settings.language.label')}
          description={t('settings.language.description')}
        >
          <NativeSelect
            id={languageId}
            aria-describedby={`${languageId}-description`}
            value={language}
            onChange={(e) => {
              const value = LANGUAGE_OPTIONS.find((option) => option === e.target.value);
              if (value) setLanguage(value);
            }}
          >
            {LANGUAGE_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {t(`settings.language.${option}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>

        <Field
          id={themeId}
          label={t('settings.theme.label')}
          description={t('settings.theme.description')}
        >
          <NativeSelect
            id={themeId}
            aria-describedby={`${themeId}-description`}
            value={theme}
            onChange={(e) => {
              const value = THEMES.find(
                (option): option is ThemePreference => option === e.target.value,
              );
              if (value) setTheme(value);
            }}
          >
            {THEMES.map((option) => (
              <option key={option} value={option}>
                {t(`settings.theme.${option}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
      </section>

      <section aria-labelledby={`${themeId}-shortcuts`} className="flex flex-col gap-3">
        <h2 id={`${themeId}-shortcuts`} className="text-sm font-semibold text-muted-foreground">
          {t('shortcuts.title')}
        </h2>
        <p className="text-xs text-muted-foreground">{t('shortcuts.description')}</p>
        <table className="w-full text-sm">
          <tbody>
            {SHORTCUTS.map((shortcut) => (
              <tr key={`${shortcut.action}-${shortcut.key}`} className="border-b last:border-0">
                <td className="py-1.5">{t(`shortcuts.actions.${shortcut.action}`)}</td>
                <td className="py-1.5 text-right">
                  {shortcutKeys(shortcut).map((part, index) => (
                    <span key={part}>
                      {index > 0 && '+'}
                      <kbd className="rounded border bg-muted px-1.5 py-0.5 font-mono text-xs">
                        {part === 'ctrl' || part === 'shift' ? t(`shortcuts.keys.${part}`) : part}
                      </kbd>
                    </span>
                  ))}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="text-xs text-muted-foreground">{t('shortcuts.pasteAlternative')}</p>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-muted-foreground">{t('settings.about.title')}</h2>
        <p className="text-sm">
          {t('app.name')} · {t('settings.about.version')}{' '}
          <span data-testid="app-version" className="font-mono">
            {version ?? t('settings.about.unavailable')}
          </span>
        </p>
      </section>
    </div>
  );
}
