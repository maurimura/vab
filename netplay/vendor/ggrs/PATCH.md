# GGRS 0.13.0, patched

The `ggrs` crate from crates.io (0.13.0), used through `[patch.crates-io]` in the workspace
Cargo.toml, with one addition in `src/sessions/p2p_session.rs`:

- `P2PSession::set_frame_delay(player_handle, delay)`: changes a local player's input delay
  while the session runs. The input queue already supported it (`InputQueue::set_frame_delay`
  returns the fill inputs, `advance_queue_head` tosses inputs when the delay drops); this exposes
  it and sends the fill inputs to the remotes. A lockstep game needs it to pick its input delay
  from how late the others' inputs actually arrive (web/emulator/worker.js), and the delay is
  each player's own, so no agreement with the others is needed.

- `UdpProtocol::on_input` (src/network/protocol.rs) keeps received inputs back to at least
  `PENDING_OUTPUT_SIZE` frames, not only `2 * max_prediction`. Each input packet is delta-encoded
  against the sender's last acknowledged input, which trails the receiver's newest by a round
  trip; the receiver needs that reference to decode. In lockstep (`max_prediction` 0) the crate
  kept only the newest input, so with a ping over a frame most packets were undecodable and
  dropped until an acknowledgement caught up: inputs arrived in clumps once a round trip, and the
  game waited between clumps however large the input delay.

Everything else is the published crate as is. To bump GGRS, take the new release and reapply
these two changes.
