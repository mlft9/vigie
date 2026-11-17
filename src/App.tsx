import { AppShell } from '@/components/layout/AppShell';
import { useApplySettings } from '@/features/settings/useApplySettings';

export function App() {
  useApplySettings();
  return <AppShell />;
}
