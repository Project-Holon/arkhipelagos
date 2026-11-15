//! arkhipelagos world generator.
//!
//! One code for every place a world is made: linked into metaxy for online
//! games, built into the eidos mod for solo games on the player's device.
//! The game's logic is compiled in (data/world), so no files are read.
//!
//! - `rng`: our own random numbers (fixed algorithm, same on every target)
//! - `data`: the logic data and the rule language
//! - `model`: the compiled logic
//! - `gen`: options, the sweep, the item fill, the world

pub mod data;
pub mod facts;
pub mod gen;
pub mod model;
pub mod rng;
pub mod species;

pub use gen::{generate, Config, PlayerConfig, World};

/// Load and compile the logic once.
pub fn model() -> Result<model::Model, String> {
    model::Model::build(&data::Data::load()?)
}
