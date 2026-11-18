import { HostsView } from '@/features/hosts/HostsView';
import { KeysView } from '@/features/keys/KeysView';
import { SettingsView } from '@/features/settings/SettingsView';
import { SftpSectionView } from '@/features/sftp/SftpSectionView';
import { SftpTabPanel } from '@/features/sftp/SftpTabPanel';
import { useShortcuts } from '@/features/shortcuts/useShortcuts';
import { SessionPanel } from '@/features/terminal/SessionPanel';
import { cn } from '@/lib/utils';
import { HOME_TAB_ID, useSessions } from '@/stores/sessions';
import { useUi } from '@/stores/ui';

import { SectionPlaceholder } from './SectionPlaceholder';
import { Sidebar } from './Sidebar';
import { TabBar } from './TabBar';

/** Mise en page principale : barre latérale + zone d'onglets (APP-01, SSH-03). */
export function AppShell() {
  const section = useUi((s) => s.activeSection);
  const tabs = useSessions((s) => s.tabs);
  const activeTabId = useSessions((s) => s.activeTabId);
  const homeActive = activeTabId === HOME_TAB_ID;
  useShortcuts();

  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TabBar />
        <main
          id="panel-home"
          role="tabpanel"
          aria-labelledby="tab-home"
          hidden={!homeActive}
          className={cn('min-h-0 flex-1 overflow-auto', !homeActive && 'hidden')}
        >
          {section === 'settings' && <SettingsView />}
          {section === 'hosts' && <HostsView />}
          {section === 'keys' && <KeysView />}
          {section === 'sftp' && <SftpSectionView />}
          {(section === 'snippets' || section === 'tunnels') && (
            <SectionPlaceholder section={section} />
          )}
        </main>
        {/* Les sessions restent montées quand leur onglet est masqué : le terminal garde son tampon. */}
        {tabs.map((tab) =>
          tab.kind === 'sftp' ? (
            <SftpTabPanel key={tab.id} tab={tab} active={tab.id === activeTabId} />
          ) : (
            <SessionPanel key={tab.id} tab={tab} active={tab.id === activeTabId} />
          ),
        )}
      </div>
    </div>
  );
}
