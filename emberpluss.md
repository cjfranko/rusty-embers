# Ember+ Integration Brief — Callie

Context handoff document. Written for another LLM/assistant picking up this work; summarizes what's been decided and what's still open.

## Background

**Callie** is a broadcast sound cart playout application, written in Rust, developed by Fetch Media Tools Limited. Callie already supports GPIO-triggered playback (GFX-triggered audio cues) and exposes **Open Sound Control (OSC) over UDP on port 9000**.

A third party (likely a broadcast console / router environment — e.g. Lawo mc², VSM, or similar) has asked whether Callie can integrate with **Ember+**, Lawo's open control protocol, so that Callie's playout can be triggered/monitored from that environment the same way other Lawo-ecosystem devices are.

## What Ember+ is (summary)

- Open control protocol from Lawo (github.com/Lawo/ember-plus), used widely in broadcast infrastructure.
- Consists of three layers:
  - **Glow** — the data schema (ASN.1-defined): a tree of Nodes, Parameters, Matrices, and Functions. No fixed vocabulary — the protocol standardizes structure, not content.
  - **EmBER** — BER (Basic Encoding Rules) encoding of Glow objects.
  - **S101** — byte-framing protocol for transmitting EmBER data over a stream transport (typically TCP).
- Model: a **provider** (device exposing controllable state — this would be Callie) and a **consumer** (controller — e.g. a Lawo console) which connects, walks/subscribes to the tree, and reads/writes/invokes.
- Default transport: **TCP port 9000**.

### S101 frame structure (single-packet example)

```
0xFE   BOF
0x00   Slot
0x0E   Message type: EmBER
0x00   Command type: EmBER Packet
0x01   Version
0xC0   Flags (single packet: first|last)
0x01   DTD type: Glow
0x02   App bytes count
0x05   Glow DTD minor version
0x02   Glow DTD major version
<payload>   -- BER-encoded Glow data
CRC lo
CRC hi
0xFF   EOF
```

- Escape byte `0xFD` (CE); any occurrence of `0xFE`, `0xFF`, or `0xFD` in the payload/CRC is escaped as `0xFD <byte XOR 0x20>`.
- CRC-16-CCITT over unescaped frame contents (excluding BOF/EOF).
- Multi-packet messages: flags byte's top bits mark first/middle/last packet fragment; reassemble before BER parsing.
- Other S101 command types: `KEEPALIVE_REQUEST` (0x01), `KEEPALIVE_RESPONSE` (0x02) — must be answered promptly or consumers treat the connection as dead.
- Glow types use custom BER `APPLICATION` tags (e.g. `GlowConnection` = `[APPLICATION 16]`, `GlowFunction` = `[APPLICATION 19]`) rather than universal ASN.1 tags.
- Reference implementation and full spec PDF: https://github.com/Lawo/ember-plus (see `documentation/Ember+ Documentation.pdf`, and `libember`/`libember_slim`/`libs101` source for exact tag numbers and encoding rules).
- Wireshark ships a full S101 dissector (`packet-s101.c`) — useful for generating/validating test vectors against real traffic.

## Rust ecosystem status

**No mature, actively-maintained Rust crate exists for Ember+/Glow/S101** (checked crates.io/docs.rs — nothing production-ready; unrelated crates coincidentally share the "ember" name). Three implementation options were considered:

1. **FFI-bind `libember_slim`** (Lawo's official lightweight C reference implementation, designed for embedded providers) via `bindgen`. Likely fastest route to a correct implementation, since BER/S101 edge cases (indefinite-length forms, escaping) are easy to get subtly wrong from scratch.
2. **Implement natively in Rust** — general BER handling via a crate like `rasn`, plus hand-written Glow-specific schema/tag layout and S101 framing. More idiomatic long-term, more upfront work.
3. **Scope down to a minimal provider-only implementation** — no matrix/function support beyond what's needed, small fixed tree. This is the direction favored given Callie's actual requirements (see below).

## Callie's actual requirement (as scoped so far)

Callie only needs to:
- **Trigger a clip** (play a specific cart)
- **Stop a clip**
- Possibly expose **status readback** (e.g. which cart is playing) — not yet confirmed whether the consumer needs this or just wants fire-and-forget triggering.

This is a small surface — not a full mixer/router integration.

### Design options discussed for "trigger"

**Option A — boolean/impulse ("pulse") parameter.** Consumer sets a boolean parameter true; provider acts, then resets it to false and pushes the change. Common convention in the Lawo world for stateless actions. Simpler consumer compatibility (more Ember+ consumers implement basic parameter read/write than implement Function invocation), but has a race-condition risk over laggy networks (did the provider reset the pulse in time / did the consumer see it).

**Option B — Glow Function (`Invoke`).** Model `PlayClip(cartId: integer)` and `Stop()` as invokable functions with typed args and a return value (success/fail, or resulting state). Semantically cleaner, matches how Callie already treats triggers as discrete events (GPIO), avoids the pulse race. Downside: fewer third-party Ember+ consumers fully support Function invocation — **needs to be confirmed against whatever the actual consumer is** (Lawo mc² console vs. VSM vs. a router panel, etc.) before committing to this approach.

No final decision has been made between A and B — pending clarification of what the requesting party's consumer software actually supports, and whether status feedback is required.

### Minimal tree sketch (illustrative, not final)

```
RootElementCollection
└── Node "Callie"
    ├── Node "Cart 1"
    │   ├── Parameter "Name"     (string, read-only)
    │   ├── Parameter "Trigger"  (boolean, pulse)  -- or Function "Play"
    │   └── Parameter "Status"   (enum: Stopped/Playing/Armed, read-only, subscribable)
    ├── Node "Cart 2" ...
    └── Function "Stop"  (no args, or optional cart id)
```

Open question: is the number of triggerable cart slots fixed, or dynamic (grows with whatever's loaded)? Dynamic carts would require handling structural tree-change notifications, adding complexity; a fixed slot count keeps the provider static and much simpler.

### Minimal provider feature set

If scoped to trigger/stop only, the provider needs to correctly handle:
- `GetDirectory` (root and per-node) → return the fixed tree
- `Subscribe` on status parameter(s) → push value-changed messages on playback state change (if status feedback is required)
- `Invoke` (if using Functions) or parameter `setValue` (if using pulse parameters) → wire into Callie's existing trigger logic
- KeepAlive request/response

Estimated to be a genuinely small implementation (order of a few hundred lines of Rust) once S101/BER plumbing exists, given the static/shallow tree.

## Port collision check: OSC (UDP/9000) vs. Ember+ (TCP/9000)

Confirmed **no conflict**: TCP and UDP are separate port namespaces at the OS/socket level, so a UDP/9000 listener (OSC) and a TCP/9000 listener (Ember+) can coexist without interference — this assumes Callie's OSC implementation is UDP-based (the overwhelmingly common default for OSC, though some implementations support TCP+SLIP framing instead — **worth explicitly verifying against Callie's actual OSC listener code**, since if OSC were TCP/9000 there would be a genuine conflict requiring a port change).

Practical notes, not blockers:
- Firewall rules (the user runs UniFi with a Policy Engine setup) are protocol+port pairs, so a separate allow rule for TCP/9000 will be needed even with UDP/9000 already open.
- Ember+'s de facto standard port is 9000 (most consumers default to trying it), so that side probably shouldn't move. Moving OSC off 9000 for clarity was floated as an option but is a judgment call, not a correctness requirement.

## Open items / next steps

1. Confirm with the requesting party whether their Ember+ consumer supports Function `Invoke`, to decide between pulse-parameter vs. function-based triggering.
2. Confirm whether status/feedback (e.g. "now playing") is actually required, or if fire-and-forget triggering is sufficient — affects whether Subscribe/push logic is needed at all.
3. Confirm whether Callie's cart slots are fixed-count or dynamic — affects tree complexity.
4. Verify Callie's existing OSC listener is UDP (not TCP) before finalizing port assignment for the Ember+ TCP listener.
5. Decide implementation approach for the Rust side: FFI-bind `libember_slim`, or hand-roll BER/S101 (leaning toward the former for correctness given no mature crate exists, per above).
6. If proceeding with hand-rolled implementation, obtain real S101/Glow packet captures (e.g. via Wireshark against a known-good Ember+ device/tool) to use as test vectors for the encoder/decoder.