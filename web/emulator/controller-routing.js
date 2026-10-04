// Some uprights have one set of game controls, one coin chute, or a second
// Start mapped onto player 1's RetroPad. Derive aliases from the core descriptors;
// keep the original 16-bit input packets unchanged for rollback and spectators.
const COIN = 1 << 2;
const START = 1 << 3;

export function controllerRouting(descriptors) {
  const first = descriptors[0] ?? new Map();
  const second = descriptors[1] ?? new Map();
  return {
    sharedCoin: first.has(2) && !second.has(2),
    secondStart: [...first].find(([, label]) => label === '2P Start')?.[0],
  };
}

export function routedButton(inputs, port, id, routing, turns = false) {
  let mask = inputs[port] ?? 0;
  // Both games that read port 1 on every turn and games that switch between
  // upright/cocktail input banks receive the shared panel's actions.
  if (turns && port < 2) mask |= inputs[1 - port] & ~(START | COIN);
  if (port === 0) {
    if (id === 2 && routing.sharedCoin) mask |= inputs[1] & COIN;
    if (id === routing.secondStart && (inputs[1] & START)) return 1;
  }
  return (mask >> id) & 1;
}
