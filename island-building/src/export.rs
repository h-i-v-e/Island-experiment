//! Create-only portable packages. A failed write never publishes a partial package.

use crate::{BuildingMesh, BuildingPrototype, BuildingRecipe, MaterialSlot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs,
    io::{self, BufWriter},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct ExportError(pub String);
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ExportError {}
impl From<io::Error> for ExportError {
    fn from(e: io::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for ExportError {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExportManifest {
    pub package_version: u32,
    pub geometry_version: u32,
    pub material_version: u32,
    pub bake_version: u32,
    pub prototype_hash: String,
    pub coordinate_system: String,
    pub normal_convention: String,
    pub mask_channels: String,
    pub door_policy: String,
    pub lod_triangles: [usize; 3],
    pub files: Vec<ExportFile>,
}

/// Preserve the public JSON shape while borrowing the large immutable buffers.
#[derive(Serialize)]
struct PortablePrototype<'a> {
    recipe: &'a BuildingRecipe,
    layout: &'a crate::layout::BuildingLayout,
    lods: &'a [crate::BuildingLod; 3],
    doors: &'a [crate::DoorDescriptor],
    colliders: &'a [crate::collision::BoxCollider],
    atlas: &'a crate::BakedAtlas,
    materials: &'a [crate::MaterialTexture],
    hash: &'a str,
}

/// Export an immutable package into a destination that must not exist.
/// Source dependencies are captured and rewritten relative to `recipe.json`.
pub fn export_package(
    prototype: &BuildingPrototype,
    destination: &Path,
    source_base: &Path,
) -> Result<ExportManifest, ExportError> {
    if destination.exists() {
        return Err(ExportError(format!(
            "{} already exists; choose a new package directory",
            destination.display()
        )));
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(
        ".building-export-{}-{}",
        std::process::id(),
        EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&staging)?;
    let result = (|| {
        let manifest = write_package(prototype, &staging, source_base)?;
        // Recheck directly before publishing. Never merge with an existing package.
        if destination.exists() {
            return Err(ExportError(
                "Export destination appeared while baking; nothing was replaced".into(),
            ));
        }
        fs::rename(&staging, destination)?;
        if let Ok(directory) = fs::File::open(parent) {
            directory.sync_all()?;
        }
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn write_package(
    prototype: &BuildingPrototype,
    root: &Path,
    source_base: &Path,
) -> Result<ExportManifest, ExportError> {
    fs::create_dir(root.join("maps"))?;
    fs::create_dir(root.join("dependencies"))?;
    let mut files = Vec::new();
    let recipe = snapshot_dependencies(prototype, root, source_base, &mut files)?;
    write_json(root, "recipe.json", &recipe, &mut files)?;
    let portable = PortablePrototype {
        recipe: &recipe,
        layout: &prototype.layout,
        lods: &prototype.lods,
        doors: &prototype.doors,
        colliders: &prototype.colliders,
        atlas: &prototype.atlas,
        materials: &prototype.materials,
        hash: &prototype.hash,
    };
    write_json(root, "prototype.json", &portable, &mut files)?;
    write_json(root, "doors.json", &prototype.doors, &mut files)?;
    write_json(root, "collision.json", &prototype.colliders, &mut files)?;
    write_json(root, "layout.json", &prototype.layout, &mut files)?;
    write_assets(prototype, root, &mut files)?;
    let manifest=ExportManifest {
        package_version:1,geometry_version:3,material_version:2,bake_version:2,
        prototype_hash:prototype.hash.clone(),coordinate_system:"right-handed, native Z-up, metres; OBJ UV V points up".into(),
        normal_convention:"OpenGL tangent-space +Y; linear normal maps".into(),mask_channels:"RGBA = occlusion, roughness, metallic, 1; linear".into(),
        door_policy:"LOD2 appearance uses closed leaves; preserve instance angles independently. door-N.obj and door-N-hardware.obj share hinge-local coordinates and map to doors.json[N]. New doors default closed; removed IDs are ignored.".into(),
        lod_triangles:std::array::from_fn(|index|prototype.lods[index].triangle_count()),files,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::write(root.join("manifest.json"), bytes)?;
    fs::File::open(root.join("manifest.json"))?.sync_all()?;
    fs::File::open(root)?.sync_all()?;
    Ok(manifest)
}

fn snapshot_dependencies(
    prototype: &BuildingPrototype,
    root: &Path,
    source_base: &Path,
    files: &mut Vec<ExportFile>,
) -> Result<BuildingRecipe, ExportError> {
    let mut recipe = prototype.recipe.clone();
    for (slot, material_slot, reference) in [
        (
            "wall",
            MaterialSlot::Wall,
            &mut recipe.materials.wall_recipe,
        ),
        (
            "roof",
            MaterialSlot::Roof,
            &mut recipe.materials.roof_recipe,
        ),
        (
            "door",
            MaterialSlot::Door,
            &mut recipe.materials.door_recipe,
        ),
    ] {
        if let Some(source) = reference.as_deref() {
            let bytes = fs::read(source_base.join(source))
                .map_err(|e| ExportError(format!("material {source}: {e}")))?;
            let hash = format!("{:x}", Sha256::digest(&bytes));
            if !prototype
                .materials
                .iter()
                .any(|material| material.slot == material_slot && material.source_hash == hash)
            {
                return Err(ExportError(format!(
                    "Material {source} changed since generation; regenerate before exporting"
                )));
            }
            let relative = format!("dependencies/{slot}.json");
            write_bytes(root, &relative, &bytes, files)?;
            *reference = Some(relative);
        }
    }
    Ok(recipe)
}

fn write_assets(
    prototype: &BuildingPrototype,
    root: &Path,
    files: &mut Vec<ExportFile>,
) -> Result<(), ExportError> {
    write_map(
        root,
        "maps/atlas-albedo.png",
        prototype.atlas.size,
        &prototype.atlas.albedo,
        files,
    )?;
    write_map(
        root,
        "maps/atlas-normal.png",
        prototype.atlas.size,
        &prototype.atlas.normal,
        files,
    )?;
    write_map(
        root,
        "maps/atlas-masks.png",
        prototype.atlas.size,
        &prototype.atlas.masks,
        files,
    )?;
    for texture in &prototype.materials {
        let name = material_name(texture.slot);
        for (suffix, bytes) in [
            ("albedo", &texture.albedo),
            ("normal", &texture.normal),
            ("masks", &texture.masks),
        ] {
            write_map(
                root,
                &format!("maps/{name}-{suffix}.png"),
                texture.size,
                bytes,
                files,
            )?;
        }
    }
    write_meshes(prototype, root, files)
}

fn write_meshes(
    prototype: &BuildingPrototype,
    root: &Path,
    files: &mut Vec<ExportFile>,
) -> Result<(), ExportError> {
    write_bytes(
        root,
        "building.mtl",
        material_library(prototype).as_bytes(),
        files,
    )?;
    for (index, lod) in prototype.lods.iter().enumerate() {
        let mut obj = String::from("# Island Building; metres, native Z-up\nmtllib building.mtl\n");
        let mut vertex_base = 1;
        for (part_index, part) in lod.parts.iter().enumerate() {
            writeln!(
                obj,
                "o lod{index}_part{part_index}\nusemtl {}",
                material_name(part.material)
            )
            .unwrap();
            append_mesh(&mut obj, &part.mesh, &mut vertex_base);
        }
        write_bytes(root, &format!("lod{index}.obj"), obj.as_bytes(), files)?;
    }
    for (index, door) in prototype.doors.iter().enumerate() {
        let mut obj = String::from(
            "# Door leaf in hinge-local coordinates; use doors.json for hinge and angles\nmtllib building.mtl\nusemtl door\n",
        );
        append_mesh(&mut obj, &door.mesh, &mut 1);
        write_bytes(root, &format!("door-{index}.obj"), obj.as_bytes(), files)?;
        let mut hardware = String::from(
            "# Door hardware shares the leaf hinge-local coordinates and animation\nmtllib building.mtl\nusemtl hardware\n",
        );
        append_mesh(&mut hardware, &door.hardware_mesh, &mut 1);
        write_bytes(
            root,
            &format!("door-{index}-hardware.obj"),
            hardware.as_bytes(),
            files,
        )?;
    }
    Ok(())
}

fn append_mesh(output: &mut String, mesh: &BuildingMesh, base: &mut usize) {
    for [x, y, z] in &mesh.positions {
        writeln!(output, "v {x} {y} {z}").unwrap();
    }
    for [u, v] in &mesh.uvs {
        writeln!(output, "vt {u} {}", 1. - v).unwrap();
    }
    for [x, y, z] in &mesh.normals {
        writeln!(output, "vn {x} {y} {z}").unwrap();
    }
    for triangle in mesh.indices.chunks_exact(3) {
        output.push('f');
        for index in triangle {
            let index = *base + *index as usize;
            write!(output, " {index}/{index}/{index}").unwrap();
        }
        output.push('\n');
    }
    *base += mesh.positions.len();
}

fn material_name(slot: MaterialSlot) -> &'static str {
    match slot {
        MaterialSlot::Wall => "wall",
        MaterialSlot::Roof => "roof",
        MaterialSlot::Floor => "floor",
        MaterialSlot::Interior => "interior",
        MaterialSlot::Trim => "trim",
        MaterialSlot::Door => "door",
        MaterialSlot::Hardware => "hardware",
        MaterialSlot::Glass => "glass",
        MaterialSlot::WindowProxy => "window_proxy",
        MaterialSlot::Atlas => "atlas",
    }
}

fn material_library(prototype: &BuildingPrototype) -> String {
    let p = &prototype.recipe.materials;
    let mut text = String::from(
        "# Normal and ORM maps are described by manifest.json; albedo maps are sRGB\n",
    );
    for (slot, color) in [
        (MaterialSlot::Wall, p.wall),
        (MaterialSlot::Roof, p.roof),
        (MaterialSlot::Floor, p.floor),
        (MaterialSlot::Interior, p.interior),
        (MaterialSlot::Trim, p.trim),
        (MaterialSlot::Door, p.door),
        (MaterialSlot::Hardware, [0.48, 0.44, 0.34]),
        (MaterialSlot::Glass, [p.glass[0], p.glass[1], p.glass[2]]),
        (MaterialSlot::WindowProxy, [0.16, 0.23, 0.25]),
        (MaterialSlot::Atlas, [1.; 3]),
    ] {
        let name = material_name(slot);
        let textured = slot == MaterialSlot::Atlas
            || prototype
                .materials
                .iter()
                .any(|texture| texture.slot == slot);
        let color = if textured { [1.; 3] } else { color };
        let (roughness, metallic) = if slot == MaterialSlot::Hardware {
            (0.24, 0.9)
        } else {
            (0.7, 0.)
        };
        writeln!(
            text,
            "newmtl {name}\nKd {} {} {}\nPr {roughness}\nPm {metallic}",
            color[0], color[1], color[2]
        )
        .unwrap();
        if slot == MaterialSlot::Glass {
            writeln!(text, "d {}\nillum 4", p.glass[3]).unwrap();
        }
        if textured {
            writeln!(
                text,
                "map_Kd maps/{name}-albedo.png\nmap_Bump maps/{name}-normal.png"
            )
            .unwrap();
        }
        text.push('\n');
    }
    text
}

fn write_json(
    root: &Path,
    name: &str,
    value: &impl Serialize,
    files: &mut Vec<ExportFile>,
) -> Result<(), ExportError> {
    write_bytes(root, name, &serde_json::to_vec_pretty(value)?, files)
}
fn write_bytes(
    root: &Path,
    name: &str,
    bytes: &[u8],
    files: &mut Vec<ExportFile>,
) -> Result<(), ExportError> {
    let path = root.join(name);
    fs::write(&path, bytes)?;
    fs::File::open(path)?.sync_all()?;
    files.push(ExportFile {
        path: name.into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        bytes: bytes.len() as u64,
    });
    Ok(())
}
fn write_map(
    root: &Path,
    name: &str,
    size: u32,
    rgba: &[u8],
    files: &mut Vec<ExportFile>,
) -> Result<(), ExportError> {
    if rgba.len() != size as usize * size as usize * 4 {
        return Err(ExportError(format!("{name}: invalid RGBA buffer")));
    }
    let mut encoder = png::Encoder::new(
        BufWriter::new(fs::File::create(root.join(name))?),
        size,
        size,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| ExportError(e.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| ExportError(e.to_string()))?;
    writer.finish().map_err(|e| ExportError(e.to_string()))?;
    let bytes = fs::read(root.join(name))?;
    fs::File::open(root.join(name))?.sync_all()?;
    files.push(ExportFile {
        path: name.into(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        bytes: bytes.len() as u64,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "building-package-{}-{}-{label}",
            std::process::id(),
            EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn package_is_portable_hashed_and_preserves_doors() {
        let target = directory("complete");
        let source = directory("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("wall.json"),
            include_bytes!("../../island-rs/texture-recipes/cracked-stone.json"),
        )
        .unwrap();
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.materials.wall_recipe = Some("wall.json".into());
        fs::write(
            source.join("door.json"),
            include_bytes!("../../island-rs/texture-recipes/wooden-planks.json"),
        )
        .unwrap();
        recipe.materials.door_recipe = Some("door.json".into());
        let prototype = crate::generate_with_base(&recipe, &source).unwrap();
        let manifest = export_package(&prototype, &target, &source).unwrap();
        fs::remove_dir_all(&source).unwrap();
        for entry in &manifest.files {
            let bytes = fs::read(target.join(&entry.path)).unwrap();
            assert_eq!(entry.sha256, format!("{:x}", Sha256::digest(&bytes)));
        }
        let loaded: crate::BuildingRecipe =
            serde_json::from_slice(&fs::read(target.join("recipe.json")).unwrap()).unwrap();
        assert_eq!(
            crate::generate_with_base(&loaded, &target).unwrap().hash,
            prototype.hash
        );
        let doors: Vec<crate::DoorDescriptor> =
            serde_json::from_slice(&fs::read(target.join("doors.json")).unwrap()).unwrap();
        assert_eq!(doors, prototype.doors);
        assert!(target.join("lod2.obj").exists());
        assert!(target.join("door-0-hardware.obj").exists());
        assert!(target.join("dependencies/door.json").exists());
        assert_eq!(
            loaded.materials.door_recipe.as_deref(),
            Some("dependencies/door.json")
        );
        assert!(
            manifest
                .files
                .iter()
                .any(|file| file.path == "maps/door-albedo.png")
        );
        assert!(export_package(&prototype, &target, Path::new(".")).is_err());
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn bad_map_cleans_staging_and_never_publishes() {
        let target = directory("failed");
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        let mut prototype = crate::generate(&recipe).unwrap();
        prototype.atlas.albedo.clear();
        assert!(export_package(&prototype, &target, Path::new(".")).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn changed_material_dependency_is_not_packaged_under_an_old_hash() {
        let target = directory("changed-dependency");
        let source = directory("changed-source");
        fs::create_dir(&source).unwrap();
        let path = source.join("wall.json");
        fs::write(
            &path,
            include_bytes!("../../island-rs/texture-recipes/cracked-stone.json"),
        )
        .unwrap();
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.materials.wall_recipe = Some("wall.json".into());
        let prototype = crate::generate_with_base(&recipe, &source).unwrap();
        fs::write(&path, b"changed by external editor").unwrap();
        let error = export_package(&prototype, &target, &source).unwrap_err();
        assert!(error.to_string().contains("changed since generation"));
        assert!(!target.exists());
        fs::remove_dir_all(source).unwrap();
    }
}
