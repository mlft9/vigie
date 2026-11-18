import { AppShell } from '@/components/layout/AppShell';
import { useApplySettings } from '@/features/settings/useApplySettings';
import { VaultGate } from '@/features/vault/VaultGate';

export function App() {
  useApplySettings();
  return (
    <VaultGate>
      <AppShell />
    </VaultGate>
  );
}
