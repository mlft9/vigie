import { ChevronDown } from 'lucide-react';
import type * as React from 'react';

import { cn } from '@/lib/utils';

/**
 * Liste déroulante native stylée façon shadcn/ui.
 * Choix délibéré plutôt que le Select de Radix : accessible et utilisable au clavier
 * nativement, et il n'injecte aucune balise <style> (compatible avec la CSP stricte).
 */
function NativeSelect({ className, children, ...props }: React.ComponentProps<'select'>) {
  return (
    <div className="relative w-full max-w-xs">
      <select
        data-slot="native-select"
        className={cn(
          'h-9 w-full appearance-none rounded-md border border-input bg-background py-1 pr-9 pl-3 text-sm shadow-xs outline-none',
          'focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50',
          'disabled:cursor-not-allowed disabled:opacity-50',
          className,
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 right-3 size-4 -translate-y-1/2 opacity-60"
      />
    </div>
  );
}

export { NativeSelect };
