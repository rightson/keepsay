//! Credential Manager-backed vault (M1).
//!
//! CRED_TYPE_GENERIC, CRED_PERSIST_LOCAL_MACHINE (never ENTERPRISE), target
//! "keepsay/anthropic". Zero the CredReadW buffer before CredFree (INV-1).
