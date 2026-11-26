import { userSettings } from '../../user-settings';
import { useEffect, useState } from 'react';

/** Window preferences belong to the user's profile, independently of the open document. */
export function useViewPreference<T extends string | boolean>(name: string, fallback: T, allowed?: readonly T[]) {
  const key = `folio.view.${name}`;
  const [value, setValue] = useState<T>(() => {
    try {
      const saved = JSON.parse(userSettings.getItem(key) ?? 'null');
      if (typeof saved === typeof fallback && (!allowed || allowed.includes(saved))) return saved as T;
    } catch { /* Use the default for an unavailable or damaged preference. */ }
    return fallback;
  });
  useEffect(() => {
    try { userSettings.setItem(key, JSON.stringify(value)); } catch { /* Storage may be unavailable. */ }
  }, [key, value]);
  return [value, setValue] as const;
}
