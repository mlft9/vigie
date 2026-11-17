import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { initI18n } from './i18n';
import { detectLanguage, systemLanguages } from './i18n/languages';
import { useSettings } from './stores/settings';
import './index.css';

// Langue initiale déterminée avant le premier rendu, pour éviter un affichage dans la mauvaise langue.
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
