import { tr } from '../../i18n';
export const EDITORIAL_PANELS = {
  notes: { get title(){return tr('메모');}, icon:'note', width:320, height:440 },
  changes: { get title(){return tr('변경 내용 추적');}, icon:'type', width:340, height:440 },
};
