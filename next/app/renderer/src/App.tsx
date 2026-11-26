import { createControls } from './app/createControls';
import { createPanels } from './app/createPanels';
import { useAppEvents } from './app/useAppEvents';
import { useFeatureMenus } from './app/useFeatureMenus';
import { useEditingCommands } from './app/useEditingCommands';
import { useViewCommands } from './app/useViewCommands';
import { useDocumentCommands } from './app/useDocumentCommands';
import { useAppState } from './app/useAppState';
import { useMenuHandlers } from './app/useMenuHandlers';
import { AppLayout } from './app/AppLayout';
import { useViewStateSync } from './app/useViewStateSync';
// Folio Studio's window: the InDesign "Essentials Classic" workspace (menu bar, Control panel,
// Tools panel, document window, panel dock) around the engine's document.
import { CLASSIC } from './ui/dock';
import './ui/workspace.css';
import './ui/panelkit.css';

export function useApp() {
  const state = useAppState();
  const documents = useDocumentCommands(state);
  const viewCommands = useViewCommands(state);
  const editing = useEditingCommands(state, documents, viewCommands);
  const getCommands = () => commandsRef.current;
  const features = useFeatureMenus(state, editing, viewCommands, documents, getCommands);
  const context = {
    ...state,
    ...documents,
    ...viewCommands,
    ...editing,
    ...features,
  };
  const commandsRef = useMenuHandlers(context);
  useViewStateSync(context);
  const commands = commandsRef.current;
  void features.panelOf;

  const events = useAppEvents(context, commandsRef);
  const panels = createPanels(context, commandsRef, commands);
  const controls = createControls(context, commands, panels.ctx);
  return { ...context, ...events, commands, ...controls, ...panels };
}

declare global {
  interface Window {
    folioWindow?: { setChrome(color: string, symbol: string): void; minimize?(): Promise<boolean> };
  }
}

void CLASSIC;

export type AppLayoutContext = ReturnType<typeof useApp>;

export function App() {
  return AppLayout(useApp());
}

