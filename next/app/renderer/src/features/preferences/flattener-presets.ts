export type FlattenerPreset = {
  name: string;
  rasterVectorBalance: number;
  lineArtAndTextResolution: number;
  gradientAndMeshResolution: number;
  clipComplexRegions: boolean;
  convertAllStrokesToOutlines: boolean;
  convertAllTextToOutlines: boolean;
};
export function cleanFlatteners(value: unknown): FlattenerPreset[] {
  if (!Array.isArray(value)) return [];
  const names = new Set<string>();
  return value.filter((p): p is FlattenerPreset => {
    if (!p || typeof p.name !== 'string' || !p.name.trim() || names.has(p.name)) return false;
    if (!Number.isFinite(p.rasterVectorBalance) || p.rasterVectorBalance < 0 || p.rasterVectorBalance > 100) return false;
    if (!Number.isFinite(p.lineArtAndTextResolution) || p.lineArtAndTextResolution < 1 || p.lineArtAndTextResolution > 9600) return false;
    if (!Number.isFinite(p.gradientAndMeshResolution) || p.gradientAndMeshResolution < 1 || p.gradientAndMeshResolution > 1200) return false;
    if (['clipComplexRegions', 'convertAllStrokesToOutlines', 'convertAllTextToOutlines'].some(k => typeof p[k] !== 'boolean')) return false;
    names.add(p.name); return true;
  }).map(p => ({ ...p }));
}
