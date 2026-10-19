//! OmniCore — an original Minecraft Java Edition client library.
//!
//! Implements the network protocol (framing, compression, encryption, login,
//! configuration and play states), world/chunk decoding, entity and inventory
//! tracking, player physics and A* navigation. It does not use Mineflayer,
//! minecraft-protocol, Prismarine or any other Minecraft bot framework.

pub mod auth;
pub mod client;
pub mod data;
pub mod entity;
pub mod inventory;
pub mod pathfind;
pub mod physics;
pub mod protocol;
pub mod world;

pub use client::{Client, ClientConfig, Event, Events, GameState, Phase, StatusInfo};
