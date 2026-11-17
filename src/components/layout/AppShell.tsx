import { SettingsView } from '@/features/settings/SettingsView';
import { useUi } from '@/stores/ui';

import { SectionPlaceholder } from './SectionPlaceholder';
import { Sidebar } from './Sidebar';
import { TabBar } from './TabBar';

/** Mise en page principale : barre latérale + zone d'onglets (APP-01). */
export function AppShell() {
  const section = useUi((s) => s.activeSection);

  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TabBar />
        <main
          id="panel-home"
          role="tabpanel"
          aria-labelledby="tab-home"
          className="min-h-0 flex-1 overflow-auto"
        >
          {section === 'settings' ? <SettingsView /> : <SectionPlaceholder section={section} />}
        </main>
      </div>
    </div>
  );
}
