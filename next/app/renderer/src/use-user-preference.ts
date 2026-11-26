import { useSyncExternalStore } from 'react';
import { userSettings } from './user-settings';

export function useUserPreference(key: string): string | null {
  return useSyncExternalStore(userSettings.subscribe, () => userSettings.getItem(key));
}
