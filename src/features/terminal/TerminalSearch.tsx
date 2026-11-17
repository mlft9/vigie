import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

interface TerminalSearchProps {
  findNext: (term: string) => boolean;
  findPrevious: (term: string) => boolean;
  onClose: () => void;
}

/** Barre de recherche dans le terminal (SSH-05) : Entrée / Maj+Entrée, Échap pour fermer. */
export function TerminalSearch({ findNext, findPrevious, onClose }: TerminalSearchProps) {
  const { t } = useTranslation();
  const [term, setTerm] = useState('');
  const [notFound, setNotFound] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  function search(direction: 'next' | 'previous') {
    if (term === '') return;
    const found = direction === 'next' ? findNext(term) : findPrevious(term);
    setNotFound(!found);
  }

  return (
    <div
      role="search"
      className="absolute top-2 right-4 z-30 flex items-center gap-1 rounded-md border bg-background p-1 shadow-md"
    >
      <Input
        ref={inputRef}
        value={term}
        aria-label={t('search.label')}
        aria-invalid={notFound}
        placeholder={t('search.placeholder')}
        className="h-8 w-56"
        onChange={(e) => {
          setTerm(e.target.value);
          setNotFound(false);
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            search(e.shiftKey ? 'previous' : 'next');
          } else if (e.key === 'Escape') {
            e.preventDefault();
            onClose();
          }
        }}
      />
      <Button
        size="icon"
        variant="ghost"
        className="size-8"
        aria-label={t('search.previous')}
        onClick={() => {
          search('previous');
        }}
      >
        <ChevronUp aria-hidden="true" />
      </Button>
      <Button
        size="icon"
        variant="ghost"
        className="size-8"
        aria-label={t('search.next')}
        onClick={() => {
          search('next');
        }}
      >
        <ChevronDown aria-hidden="true" />
      </Button>
      <Button
        size="icon"
        variant="ghost"
        className="size-8"
        aria-label={t('search.close')}
        onClick={onClose}
      >
        <X aria-hidden="true" />
      </Button>
      {notFound && (
        <span role="status" className="sr-only">
          {t('search.notFound')}
        </span>
      )}
    </div>
  );
}
