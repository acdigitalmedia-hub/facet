//! Facet's game rules: data, pathfinding, simulation and saves. No rendering here, so any
//! front end (the 3D Bevy client, headless tests) can drive it.

pub mod bot;
pub mod color;
pub mod data;
pub mod game;
pub mod path;
pub mod rng;
pub mod save;
