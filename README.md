# rusty-embers

A general-purpose Rust crate for the [Lawo Ember+](https://github.com/Lawo/ember-plus) control protocol, used in professional broadcast and audio/video infrastructure.

Ember+ is an open protocol that lets a **provider** (a device or application) expose a tree of controllable parameters to a **consumer** (a control panel, automation system, or broadcast console).

## Protocol stack

Ember+ is built in three layers:

- **Glow** — the ASN.1-defined data schema: a tree of Nodes, Parameters, Matrices, Functions, and more.
- **EmBER** — ASN.1 BER encoding of Glow objects.
- **S101** — byte-framing protocol for transmitting EmBER data over TCP (typically port 9000).

## Goals

- Provide a safe, idiomatic Rust API for building Ember+ providers and consumers.
- Keep the crate independent of any specific application or device domain.
- Offer a fast path to correctness by leveraging Lawo's official `libember_slim` reference implementation for BER/Glow encoding and decoding, while implementing S101 framing, networking, and the provider state machine in Rust.

## Approach

This crate uses a hybrid strategy:

- **FFI to `libember_slim`** (via `bindgen`) for EmBER/Glow encode/decode.
- **Native Rust** for S101 framing, the Tokio-based TCP server, and the provider state machine.

This avoids reimplementing BER edge cases (indefinite-length containers, explicit tagging, real-number encoding, 64-bit integers) while keeping the public API and async runtime idiomatic and memory-safe.

## Status

Early development. The project currently consists of planning and design; no provider or consumer implementation is available yet.

The first milestone targets **provider-only** support with a fixed, shallow object tree:

- `GetDirectory` discovery
- `Subscribe` / `Unsubscribe` for value changes
- Parameter writes (pulse parameters)
- Optional Glow Function `Invoke` support
- KeepAlive handling

Consumer/client support and dynamic trees are planned for later milestones.

## License

This crate is licensed under the MIT License. See [LICENSE](LICENSE) for details.

The upstream `libember_slim` reference implementation is licensed under the Boost Software License 1.0.
