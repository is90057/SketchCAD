//! Safe Rust API over the bundled OpenSKP SketchUp reader / writer.
//!
//! Coordinates are SketchUp model units as exposed by OpenSKP; see [`read_skp`] / [`write_skp`].

mod ffi;

pub use ffi::{read_skp, write_skp, SkpFace, SkpGroup, SkpMaterial, SkpMesh, SkpModel, SkpWriteReport};
