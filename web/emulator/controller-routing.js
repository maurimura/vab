// Some uprights have one set of game controls, one coin chute, or a second
// Start mapped onto player 1's RetroPad. Derive aliases from the core descriptors;
// keep the original input packets unchanged for rollback and spectators. Only a
// port's RetroPad mask, its low 16 bits, is read here (a lightgun's aim is above).
const PAD = 0xffff;
const COIN = 1 << 2;
const START = 1 << 3;
const UP = 4;
const DOWN = 5;
const L2 = 12;
const R2 = 13;

/**
 * `descriptors`: per port, the RetroPad buttons the core names (id -> name). (A driving game's
 * wheel is the game's to say, not the descriptors': libretro.js `wheel`.)
 */
export function controllerRouting(descriptors) {
  const first = descriptors[0] ?? new Map();
  const second = descriptors[1] ?? new Map();
  return {
    sharedCoin: first.has(2) && !second.has(2),
    secondStart: [...first].find(([, label]) => label === '2P Start')?.[0],
    // FBNeo's driving games (Out Run) put the pedals on R2 (Accelerate) and L2 (Brake), which no
    // key presses, and use neither Up nor Down: Up and Down press them instead, per port.
    pedals: descriptors.map((port) => (port.has(R2) || port.has(L2)) && !port.has(UP) && !port.has(DOWN)),
  };
}

export function routedButton(inputs, port, id, routing, turns = false) {
  let mask = (inputs[port] ?? 0) & PAD;
  // Both games that read port 1 on every turn and games that switch between
  // upright/cocktail input banks receive the shared panel's actions.
  if (turns && port < 2) mask |= inputs[1 - port] & PAD & ~(START | COIN);
  if (port === 0) {
    if (id === 2 && routing.sharedCoin) mask |= inputs[1] & COIN;
    if (id === routing.secondStart && (inputs[1] & START)) return 1;
  }
  if (routing.pedals?.[port]) {
    if (id === R2) mask |= ((mask >> UP) & 1) << R2;
    if (id === L2) mask |= ((mask >> DOWN) & 1) << L2;
  }
  return (mask >> id) & 1;
}
