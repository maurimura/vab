// Some uprights have one set of game controls, one coin chute, or a second
// Start mapped onto player 1's RetroPad. Derive aliases from the core descriptors;
// keep the original input packets unchanged for rollback and spectators. Only a
// port's RetroPad mask, its low 16 bits, is read here (a lightgun's aim is above).
const PAD = 0xffff;
const COIN = 1 << 2;
const START = 1 << 3;
const UP = 4;
const DOWN = 5;
const LEFT = 6;
const RIGHT = 7;
const L2 = 12;
const R2 = 13;
/** The left stick's X axis among a port's analog controls (libretro.js: "index/id"). */
const LEFT_X = '0/0';

/**
 * `descriptors`: per port, the RetroPad buttons the core names (id -> name); `analog`: per
 * port, the analog controls it names ("index/id" -> name).
 */
export function controllerRouting(descriptors, analog = []) {
  const first = descriptors[0] ?? new Map();
  const second = descriptors[1] ?? new Map();
  // FBNeo's driving games (Out Run) put the pedals on R2 (Accelerate) and L2 (Brake), which no
  // key presses, and use neither Up nor Down: Up and Down press them instead, per port.
  const pedals = descriptors.map((port) => (port.has(R2) || port.has(L2)) && !port.has(UP) && !port.has(DOWN));
  return {
    sharedCoin: first.has(2) && !second.has(2),
    secondStart: [...first].find(([, label]) => label === '2P Start')?.[0],
    pedals,
    // ...and steer with the left stick's X, a wheel: the arrows turn it (wheel.js), instead of
    // FBNeo's "fake digital" Left and Right, which put it at full lock at once.
    wheel: pedals.map((driving, port) => driving && Boolean(analog[port]?.has(LEFT_X))),
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
  if (routing.wheel?.[port] && (id === LEFT || id === RIGHT)) return 0;
  return (mask >> id) & 1;
}
