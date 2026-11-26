import type { TextProperties } from '../engine';

/** Shared by the character controls and the font menu. */
export function fontFamilyProperties(fonts: Record<string, string[]>, family: string, currentStyle: string): TextProperties {
  const styles = fonts[family] ?? [];
  const style = styles.includes(currentStyle) ? currentStyle : styles.includes('Regular') ? 'Regular' : styles[0];
  return { appliedFont: family, fontStyle: style || 'Regular' };
}
