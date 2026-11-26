// Object ▸ Transform ▸ Move…, Scale…, Rotate… and Shear…: the engine session holds these dialogs (next/shared/dialogs.json, the
// `move`, `scale`, `rotate` and `shear` entries; OK and Copy are its two actions) and ui/dialog-frame.tsx draws them. The engine does the
// work (folio-edit: move, scale, rotate, shear; Copy is a duplicate and the transformation of the copies in one undo step, `"@copies"`),
// measured in next/oracle/scenarios/basic-transform-again.json and basic-transform-shear.json. Everything turns about the control panel's
// reference point, as InDesign's dialogs do (the screen passes it as the dialog's `anchor`); the dialogs also feed Object ▸ Transform
// Again. Only the horizontal shear axis is measured.
export type TransformKind = 'move' | 'scale' | 'rotate' | 'shear';
