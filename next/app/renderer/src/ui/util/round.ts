/** Rounds to four decimal places without changing JavaScript's signed-zero behavior. */
export const round4 = (value: number) => Math.round(value * 1e4) / 1e4;
