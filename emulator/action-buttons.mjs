// Libretro RetroPad action IDs (B, Y, A, X, L, R, L2, R2), excluding coins,
// Start, directions and L3/R3 system controls. Preserve the core's action order:
// MK reports named punches/kicks, whereas Neo Geo reports Button A/B/C/D.
const ACTION_IDS = new Set([0, 1, 8, 9, 10, 11, 12, 13]);

export function actionButtons(buttons) {
  return [...buttons].filter(([id]) => ACTION_IDS.has(id));
}
