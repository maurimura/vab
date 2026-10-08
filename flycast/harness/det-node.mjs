// The harness's det phase (index.html ?det=N) in Node: from a state, N frames of inputs.mjs's
// random inputs, the RAM hash every 60 frames. Same hashes in both = the browser's WebGL renderer
// and Node's no-op GL leave the machine the same.
//   node flycast/harness/det-node.mjs <state> [frames=1200]
import { readFileSync } from "node:fs";
import { boot } from "../check.mjs";
import { hash, inputs } from "../inputs.mjs";

const [state, frames = "1200"] = process.argv.slice(2);
const { core } = await boot();
core.unserialize(readFileSync(state));
const script = inputs(Number(frames), 1);
const hashes = [];
for (let i = 0; i < script.length; i++) {
  core.inputs[0] = script[i][0];
  core.inputs[1] = script[i][1];
  core.run();
  if (i % 60 === 59) hashes.push(hash(core.systemRam()));
}
console.log(`det hashes: ${hashes.join(" ")}`);
