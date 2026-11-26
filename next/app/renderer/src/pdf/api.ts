import type { PdfPreset } from './types';
import { t as tr } from '../i18n';
/** A PDF preset's name as the screen shows it (the built-in ones translated). */
export function presetName(p: PdfPreset) {
  const names: Record<string, string> = { '[High Quality Print]': tr('[고품질 인쇄]'), '[Press Quality]': tr('[인쇄 품질]'), '[Smallest File Size]': tr('[최소 파일 크기]') };
  return names[p.name] ?? p.name;
}
