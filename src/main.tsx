import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { initI18n } from './i18n';
import { detectLanguage, systemLanguages } from './i18n/languages';
import { loadSettings, useSettings } from './stores/settings';
import './index.css';

async function start() {
  // Paramètres lus dans settings.json (géré par le cœur) avant le premier rendu : la langue est
  // fixée tout de suite, pour éviter un affichage dans la mauvaise langue.
  await loadSettings();
  const preference = useSettings.getState().language;
  initI18n(preference === 'system' ? detectLanguage(systemLanguages()) : preference);

  const container = document.getElementById('root');
  if (container) {
    createRoot(container).render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  }
}

void start();
