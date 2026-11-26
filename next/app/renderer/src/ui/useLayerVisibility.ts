import { useEffect, useState } from 'react';
import type { EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';

/** Keep the eye's requested state visible while the document information is refreshed. */
export function useLayerVisibility({layers, document, edit: commit}: {
  layers: DocumentInfo['layers'] | undefined; document: string | undefined;
  edit: (label: string, commands: EditCommand[]) => unknown;
}) {
  const [pending, setPending] = useState<Record<string, {visible:boolean}>>({});
  useEffect(() => setPending({}), [document]);
  useEffect(() => {
    setPending(current => {
      const rest = Object.fromEntries(Object.entries(current).filter(([id, value]) => {
        const layer = layers?.find(l => l.id === id);
        return layer && layer.visible !== value.visible;
      }));
      return Object.keys(rest).length === Object.keys(current).length ? current : rest;
    });
  }, [layers]);
  const edit = async (label: string, commands: EditCommand[]) => {
    const changes: typeof pending = {};
    for (const command of commands) {
      if (command.op === 'setLayer' && command.visible !== undefined) changes[command.id] = {visible:command.visible};
    }
    setPending(current => ({...current, ...changes}));
    const rollback = () => setPending(current => Object.fromEntries(Object.entries(current).filter(([id, value]) => changes[id] !== value)));
    try {
      const result = await commit(label, commands);
      if (!result) rollback();
      return result;
    } catch (error) { rollback(); throw error; }
  };
  return {
    layers: layers?.map(layer => pending[layer.id] ? {...layer, visible:pending[layer.id].visible} : layer),
    edit,
  };
}
