/** `MixedInkGroup/<name>` (a % written %25) as the name the panel shows. */
export const mixedGroupName = (id: string) => id.replace(/^MixedInkGroup\//, '').replace(/%25/g, '%');
