import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';

import {
  characterCount,
  estimateStrength,
  MIN_MASTER_LENGTH,
  type StrengthScore,
} from './strength';

const BAR_COLORS: Record<StrengthScore, string> = {
  0: 'bg-destructive',
  1: 'bg-destructive',
  2: 'bg-amber-500',
  3: 'bg-emerald-600',
  4: 'bg-emerald-600',
};

interface PasswordStrengthProps {
  id: string;
  password: string;
  /** Reçoit le score calculé (pour activer ou non le bouton de validation). */
  onScore: (score: StrengthScore) => void;
}

/** Indicateur de robustesse du mot de passe maître (VLT-01). Indicatif : le cœur fait foi. */
export function PasswordStrength({ id, password, onScore }: PasswordStrengthProps) {
  const { t } = useTranslation();
  const [score, setScore] = useState<StrengthScore>(0);

  useEffect(() => {
    let cancelled = false;
    // Petit délai : pas d'estimation à chaque frappe rapide.
    const timer = setTimeout(() => {
      estimateStrength(password)
        .then((value) => {
          if (cancelled) return;
          setScore(value);
          onScore(value);
        })
        .catch(() => undefined);
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [password, onScore]);

  const tooShort = characterCount(password) < MIN_MASTER_LENGTH;
  const shown: StrengthScore = password === '' ? 0 : score;

  return (
    <div id={id} className="flex flex-col gap-1.5" aria-live="polite">
      <div className="flex gap-1" aria-hidden="true">
        {[1, 2, 3, 4].map((level) => (
          <span
            key={level}
            className={cn(
              'h-1.5 flex-1 rounded-full bg-muted',
              password !== '' && shown >= level && BAR_COLORS[shown],
            )}
          />
        ))}
      </div>
      <p className="text-xs text-muted-foreground" data-testid="password-strength">
        {password === ''
          ? t('vault.strength.empty', { min: MIN_MASTER_LENGTH })
          : tooShort
            ? t('vault.strength.tooShort', { min: MIN_MASTER_LENGTH })
            : t(`vault.strength.score${shown}`)}
      </p>
    </div>
  );
}
