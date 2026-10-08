// A save state deflated for the trip through the site, as web/emulator/worker.js packs the ones
// it sends and unpacks the ones it gets (start-up states among them): "vabz", then the
// deflate-raw bytes. For the tools here, in Node: snapshot.mjs writes states so, and the checks
// take either kind.
import { deflateRawSync, inflateRawSync } from "node:zlib";

const MARK = Uint8Array.of(0x76, 0x61, 0x62, 0x7a); // "vabz"

/** `state` deflated and marked. */
export function packState(state) {
  const packed = deflateRawSync(state, { level: 9 });
  const out = new Uint8Array(MARK.length + packed.length);
  out.set(MARK);
  out.set(packed, MARK.length);
  return out;
}

/** The state back from `packState`; bytes not marked as packed are taken as they are. */
export function unpackState(bytes) {
  if (bytes.length < MARK.length || MARK.some((b, i) => bytes[i] !== b)) return bytes;
  return new Uint8Array(inflateRawSync(bytes.subarray(MARK.length)));
}
