import type { useAppState } from './useAppState';
import type { useDocumentCommands } from './useDocumentCommands';
import type { useViewCommands } from './useViewCommands';
import type { useEditingCommands } from './useEditingCommands';
import type { useFeatureMenus } from './useFeatureMenus';

export type AppContext = ReturnType<typeof useAppState>
  & ReturnType<typeof useDocumentCommands>
  & ReturnType<typeof useViewCommands>
  & ReturnType<typeof useEditingCommands>
  & ReturnType<typeof useFeatureMenus>;
