import { ShieldAlert, ShieldCheck } from 'lucide-react';
import { type SyntheticEvent, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Modal } from '@/components/ui/modal';
import type { AuthPromptInfo } from '@/lib/types';

import { ErrorMessage } from './ErrorMessage';

/** Longueur maximale d'une réponse, en octets UTF-8 (même limite que le cœur). */
const MAX_ANSWER_BYTES = 1024;

interface AuthPromptDialogProps {
  /** Adresse affichée (`hôte` ou `hôte:port`) : rappelle qui pose les questions. */
  address: string;
  prompt: AuthPromptInfo;
  /** Refus des réponses précédentes par le cœur (la question est toujours en attente). */
  errorCode?: string | null;
  onSubmit: (answers: string[]) => void;
  onCancel: () => void;
}

/**
 * Questions du serveur en authentification keyboard-interactive (AUTH-03) : code TOTP, mot de
 * passe PAM… Le texte vient du serveur (déjà nettoyé par le cœur) : il est affiché comme du
 * texte brut, dans un encadré qui le distingue de l'interface de Younji, avec un rappel contre
 * l'hameçonnage. Les réponses restent dans l'état local et sont vidées dès l'envoi.
 */
export function AuthPromptDialog({
  address,
  prompt,
  errorCode = null,
  onSubmit,
  onCancel,
}: AuthPromptDialogProps) {
  const { t } = useTranslation();
  const id = useId();
  const [answers, setAnswers] = useState<string[]>(() => prompt.prompts.map(() => ''));
  const [localError, setLocalError] = useState<string | null>(null);

  function submit(event: SyntheticEvent) {
    event.preventDefault();
    const encoder = new TextEncoder();
    if (answers.some((answer) => encoder.encode(answer).length > MAX_ANSWER_BYTES)) {
      setLocalError('ssh.invalid_answers');
      return;
    }
    setLocalError(null);
    const sent = [...answers];
    setAnswers(prompt.prompts.map(() => ''));
    onSubmit(sent);
  }

  const hasServerText = prompt.name.trim() !== '' || prompt.instructions.trim() !== '';

  return (
    <Modal labelledBy={`${id}-title`} describedBy={`${id}-description`} onClose={onCancel}>
      <form onSubmit={submit} noValidate className="flex flex-col gap-4">
        <h2 id={`${id}-title`} className="flex items-center gap-2 text-lg font-semibold">
          <ShieldCheck aria-hidden="true" className="size-5" />
          {t('authPrompt.title')}
        </h2>
        <p id={`${id}-description`} className="text-sm">
          {t('authPrompt.intro', { host: address })}
        </p>

        {(localError ?? errorCode) && <ErrorMessage code={localError ?? errorCode ?? ''} />}

        {hasServerText && (
          <figure className="flex flex-col gap-1 rounded-md border bg-muted/40 p-3 text-sm">
            <figcaption className="text-xs text-muted-foreground">
              {t('authPrompt.serverMessage')}
            </figcaption>
            {prompt.name.trim() !== '' && <p className="font-medium">{prompt.name}</p>}
            {prompt.instructions.trim() !== '' && (
              <p className="break-words whitespace-pre-wrap">{prompt.instructions}</p>
            )}
          </figure>
        )}

        {prompt.prompts.map((field, index) => {
          const label = field.text.trim();
          return (
            <div key={index} className="flex flex-col gap-1.5">
              <label htmlFor={`${id}-answer-${index}`} className="text-sm font-medium break-words">
                {label === '' ? t('authPrompt.answer', { number: index + 1 }) : label}
              </label>
              <Input
                id={`${id}-answer-${index}`}
                type={field.echo ? 'text' : 'password'}
                value={answers[index] ?? ''}
                onChange={(e) => {
                  const next = [...answers];
                  next[index] = e.target.value;
                  setAnswers(next);
                }}
                autoComplete="off"
                spellCheck={false}
                maxLength={1024}
                {...(index === 0 ? { 'data-autofocus': true } : {})}
              />
            </div>
          );
        })}

        <p className="flex gap-2 text-xs text-muted-foreground">
          <ShieldAlert aria-hidden="true" className="size-4 shrink-0" />
          {t('authPrompt.warning')}
        </p>

        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
          <Button type="submit">{t('authPrompt.submit')}</Button>
        </div>
      </form>
    </Modal>
  );
}
