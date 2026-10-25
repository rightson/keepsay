//! `uniffi-bindgen-swift`: generates keepsay-core's Swift bindings in
//! library mode from a compiled keepsay-core library (docs/changes/0019).
//! A build tool only: it is never linked into an app or a release artifact
//! and makes no network request.

fn main() {
    uniffi::uniffi_bindgen_swift();
}
