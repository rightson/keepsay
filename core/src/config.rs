//! Non-secret settings, stored as JSON with a schema version (PROPOSAL §10).
//! Never holds the key (INV-1). The glossary is private: never log it.

pub const SCHEMA_VERSION: u32 = 1;
