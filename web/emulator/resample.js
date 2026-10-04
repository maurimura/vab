// Brings a core's sound to the speaker's rate: the page's AudioContext runs at 48 kHz, which the
// FBNeo cores produce, while Supermodel's Model 3 makes 44.1 kHz. Linear interpolation between
// neighbouring samples, carrying the position across chunks so frames join without a seam.
export class Resampler {
  #step;
  /** Source position of the next output, in frames of the current chunk; -1 is the last frame of the previous one. */
  #position = 0;
  #lastLeft = 0;
  #lastRight = 0;

  constructor(fromRate, toRate) {
    this.#step = fromRate / toRate;
  }

  /** Interleaved stereo 16-bit samples in, the same at the speaker's rate out. */
  process(samples) {
    const frames = samples.length >> 1;
    if (!frames) return samples;
    const out = new Int16Array(Math.ceil((frames + 1) / this.#step) * 2);
    const at = (frame, channel) =>
      frame < 0 ? (channel ? this.#lastRight : this.#lastLeft) : samples[frame * 2 + channel];
    let n = 0;
    let position = this.#position;
    // Each output sits between source frames i and i + 1; the last frame waits for the next chunk.
    while (position + 1 < frames) {
      const i = Math.floor(position);
      const t = position - i;
      out[n++] = at(i, 0) + (at(i + 1, 0) - at(i, 0)) * t;
      out[n++] = at(i, 1) + (at(i + 1, 1) - at(i, 1)) * t;
      position += this.#step;
    }
    this.#position = position - frames;
    this.#lastLeft = samples[(frames - 1) * 2];
    this.#lastRight = samples[(frames - 1) * 2 + 1];
    return out.subarray(0, n);
  }
}
