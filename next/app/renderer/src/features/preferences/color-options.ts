import { useEffect, useState } from 'react';
export type ColorProfile = { name: string; space: 'RGB' | 'CMYK' | 'GRAY' };
export function useColorProfiles() {
  const [profiles, setProfiles] = useState<ColorProfile[]>([]);
  const [error, setError] = useState('');
  useEffect(() => {
    let alive = true;
    window.folio.request({ cmd: 'colorProfiles' }).then(r => {
      if (!r.header.ok) throw Error(String(r.header.error));
      if (alive) setProfiles(r.header.profiles as ColorProfile[]);
    }).catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, []);
  return { profiles, error };
}
