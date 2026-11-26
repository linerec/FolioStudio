/** The engine's trap preset (model/src/traps.rs). In a command a missing field keeps the value (edit) or takes InDesign's default (new). */
export type TrapPreset = {
  id?: string; name?: string;
  default_width?: number | null; black_width?: number | null;
  join?: string; end?: string;
  objects_to_images?: boolean | null; images_to_images?: boolean | null; internal_images?: boolean | null; one_bit_images?: boolean | null;
  image_placement?: string;
  step_threshold?: number | null; black_color_threshold?: number | null; black_density?: number | null;
  sliding_threshold?: number | null; color_reduction?: number | null;
};
export type TrapAssignment = { page: string; preset: string };
export type TrapSettings = { presets: TrapPreset[]; assignments: TrapAssignment[] };
