//! Camera-centred spherical ocean geometry and planet-wide coastal fields.
mod coast;
mod mesh;
pub use coast::{CoastMap, cube_direction, cube_pixel};
pub use mesh::{OceanGrid, OceanGridOptions};
