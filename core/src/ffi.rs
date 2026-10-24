//! UniFFI exports (PROPOSAL §6.1). No exported function may return the key;
//! only `vault::Fingerprint` crosses this boundary (INV-2).
//!
//! Deferred (docs/changes/0005): `uniffi` is MPL-2.0, which `deny.toml` does
//! not allow yet, and the export layer depends on every M1 module. It is
//! wired after the parallel workstreams land (US-M1-22). Until then this file
//! is frozen.
