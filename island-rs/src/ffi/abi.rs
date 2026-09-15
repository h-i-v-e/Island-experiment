//! Scalar ABI handshake: safe to call before the host trusts any struct layout.
use super::{
    ExportMesh, ExportMeshGrid, MotuFernOptions, MotuForestOptions, MotuOptions, MotuReedOptions,
    TriangleExportArray, Vector3ExportArray,
};
use std::mem::{offset_of, size_of};

/// Increment when field meanings, layouts, or ownership contracts change incompatibly.
#[unsafe(no_mangle)]
pub extern "C" fn GetMotuAbiVersion() -> u32 {
    1
}

/// Zero means unknown type. IDs are part of ABI version 1.
#[unsafe(no_mangle)]
pub extern "C" fn GetMotuAbiTypeSize(type_id: u32) -> u32 {
    let bytes = match type_id {
        0 => size_of::<MotuOptions>(),
        1 => size_of::<MotuForestOptions>(),
        2 => size_of::<MotuReedOptions>(),
        3 => size_of::<MotuFernOptions>(),
        4 => size_of::<Vector3ExportArray>(),
        5 => size_of::<TriangleExportArray>(),
        6 => size_of::<ExportMesh>(),
        7 => size_of::<ExportMeshGrid>(),
        _ => return 0,
    };
    u32::try_from(bytes).expect("ABI records fit in u32")
}

/// Forest's mixed byte/float fields exercise natural C padding on each target.
#[unsafe(no_mangle)]
pub extern "C" fn GetMotuAbiForestOffset(field_id: u32) -> u32 {
    let offset = match field_id {
        0 => offset_of!(MotuForestOptions, snowlineMetres),
        1 => offset_of!(MotuForestOptions, prototypeCount),
        2 => offset_of!(MotuForestOptions, minimumScale),
        3 => offset_of!(MotuForestOptions, fallenLogDensity),
        _ => return u32::MAX,
    };
    u32::try_from(offset).expect("ABI offsets fit in u32")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_one_preserves_generation_and_forest_layout() {
        assert_eq!(GetMotuAbiVersion(), 1);
        assert_eq!(GetMotuAbiTypeSize(0), 80);
        assert_eq!(GetMotuAbiTypeSize(1), 32);
        assert_eq!(GetMotuAbiTypeSize(99), 0);
        assert_eq!(
            [0, 1, 2, 3].map(|id| GetMotuAbiForestOffset(id)),
            [12, 16, 20, 28]
        );
        assert_eq!(GetMotuAbiForestOffset(99), u32::MAX);
    }
}
