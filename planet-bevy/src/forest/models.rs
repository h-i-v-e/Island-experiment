//! Engine-neutral Planet silhouettes are shared with Island's instanced forest.
use motu::{Mesh, TreeMeshes};

pub(super) fn conifer(variant: usize) -> TreeMeshes {
    motu::generate_conifer(variant)
}
pub(super) fn palm(variant: usize) -> TreeMeshes {
    motu::generate_palm(variant)
}
pub(super) fn broadleaf_crown(source: &Mesh) -> Mesh {
    motu::broadleaf_crown(source)
}
