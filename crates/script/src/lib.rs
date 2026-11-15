//! Field scripts of Pokemon Black and White: disassemble, edit, assemble,
//! for all 12 cartridges.
//!
//! - `table`: the command table per language family, derived from
//!   aletheia's source and checked against every cartridge's scripts.
//! - `script`: script members (commands, labels, data).
//! - `zone`: zone trigger tables (the other kind of a/0/5/7 member).
//!
//! The edit model never moves existing bytes: a change extends the entry
//! table (which shifts everything after it by the same amount, so relative
//! offsets stay valid), changes commands in place, or appends new code at
//! the end.

pub mod script;
pub mod table;
pub mod zone;

pub use script::{assemble, classify, disassemble, from_text, to_text, Error, Item, Kind, Script, Value};
pub use table::{ArgKind, Family, Table};
