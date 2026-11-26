import { createRoot } from 'react-dom/client';
// the design tokens first (ui/tokens.css, contract next/app/DESIGN.md), then the base styles: the workspace styles (imported by App) refine them
import './ui/tokens.css';
import './styles.css';
import { App } from './App';
// last: the states and motion layer (press, focus, where a drag lands, the few motions kept) lines the states up over every surface
import './ui/motion.css';
import { setLanguage } from './i18n';
import { initializeUserSettings } from './user-settings';
import { followLanguageForEngineMessages } from './i18n/engine-locale';

followLanguageForEngineMessages();

initializeUserSettings().then(() => window.folio.settings()).then(s => setLanguage(s.language ?? 'system', s.systemLanguage), () => {})
  .finally(() => createRoot(document.getElementById('root')!).render(<App />));
