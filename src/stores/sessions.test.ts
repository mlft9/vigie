import { defaultTabTitle, HOME_TAB_ID, MAX_TAB_TITLE, useSessions } from './sessions';

const store = () => useSessions.getState();
const ids = () => store().tabs.map((t) => t.id);
const firstTab = () => {
  const tab = store().tabs[0];
  if (!tab) throw new Error('aucun onglet');
  return tab;
};

describe('store des onglets de session', () => {
  beforeEach(() => {
    useSessions.setState({ tabs: [], activeTabId: HOME_TAB_ID });
  });

  it('ouvre un onglet de connexion et l’active', () => {
    const id = store().openTab();
    expect(store().activeTabId).toBe(id);
    expect(store().tabs[0]).toMatchObject({ id, status: 'form', target: null });
  });

  it('active le voisin à la fermeture de l’onglet actif', () => {
    const a = store().openTab();
    const b = store().openTab();
    const c = store().openTab();
    store().activate(b);
    store().removeTab(b);
    expect(store().activeTabId).toBe(c);
    store().removeTab(c);
    expect(store().activeTabId).toBe(a);
    store().removeTab(a);
    expect(store().activeTabId).toBe(HOME_TAB_ID);
  });

  it('parcourt les onglets en boucle, accueil compris', () => {
    const a = store().openTab();
    const b = store().openTab();
    store().activateRelative(1);
    expect(store().activeTabId).toBe(HOME_TAB_ID);
    store().activateRelative(1);
    expect(store().activeTabId).toBe(a);
    store().activateRelative(-1);
    store().activateRelative(-1);
    expect(store().activeTabId).toBe(b);
  });

  it('renomme, rétablit le titre par défaut et borne la longueur', () => {
    const id = store().openTab();
    store().update(id, {
      target: { host: 'srv', port: 22, username: 'root', authKind: 'password', keyFile: null },
    });
    expect(defaultTabTitle(firstTab())).toBe('root@srv');
    store().renameTab(id, '  Prod  ');
    expect(defaultTabTitle(firstTab())).toBe('Prod');
    store().renameTab(id, '   ');
    expect(defaultTabTitle(firstTab())).toBe('root@srv');
    store().renameTab(id, 'x'.repeat(200));
    expect(firstTab().customTitle).toHaveLength(MAX_TAB_TITLE);
  });

  it('affiche une adresse IPv6 entre crochets', () => {
    const id = store().openTab();
    store().update(id, {
      target: { host: '::1', port: 22, username: 'u', authKind: 'password', keyFile: null },
    });
    expect(defaultTabTitle(firstTab())).toBe('u@[::1]');
  });

  it('réordonne les onglets sans sortir des bornes', () => {
    const a = store().openTab();
    const b = store().openTab();
    const c = store().openTab();
    store().moveTab(a, 1);
    expect(ids()).toEqual([b, a, c]);
    store().moveTab(b, -1);
    expect(ids()).toEqual([b, a, c]);
    store().moveTabTo(c, b);
    expect(ids()).toEqual([c, b, a]);
  });

  it("n'active pas un onglet inconnu", () => {
    store().activate('inconnu');
    expect(store().activeTabId).toBe(HOME_TAB_ID);
  });
});
