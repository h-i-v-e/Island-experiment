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
pub struct MaskedMaterial {
    pub name: String,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
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
    #[serde(default)]
    pub terrain_policy: String,
    #[serde(default)]
    pub floor_policy: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masked_materials: Vec<MaskedMaterial>,
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
    support_colliders: &'a [crate::collision::BoxCollider],
    stair_colliders: &'a [crate::collision::BoxCollider],
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
    write_grub_filter(root, &recipe, &mut files)?;
    let portable = PortablePrototype {
        recipe: &recipe,
        layout: &prototype.layout,
        lods: &prototype.lods,
        doors: &prototype.doors,
        colliders: &prototype.colliders,
        support_colliders: &prototype.support_colliders,
        stair_colliders: &prototype.stair_colliders,
        atlas: &prototype.atlas,
        materials: &prototype.materials,
        hash: &prototype.hash,
    };
    write_json(root, "prototype.json", &portable, &mut files)?;
    write_json(root, "doors.json", &prototype.doors, &mut files)?;
    write_json(root, "collision.json", &prototype.colliders, &mut files)?;
    write_json(
        root,
        "support-collision.json",
        &prototype.support_colliders,
        &mut files,
    )?;
    write_json(
        root,
        "stairs-collision.json",
        &prototype.stair_colliders,
        &mut files,
    )?;
    write_json(
        root,
        "stairs.json",
        &crate::geometry::foundation::stairs(&prototype.recipe).collect::<Vec<_>>(),
        &mut files,
    )?;
    write_json(root, "layout.json", &prototype.layout, &mut files)?;
    write_json(
        root,
        "foundation.json",
        &crate::geometry::foundation::bounds(&prototype.recipe),
        &mut files,
    )?;
    write_assets(prototype, root, &mut files)?;
    let masked_materials = prototype
        .materials
        .iter()
        .filter(|texture| texture.slot.thatch_shell_layer().is_some())
        .map(|texture| MaskedMaterial {
            name: material_name(texture.slot).into(),
            alpha_cutoff: 0.5,
            double_sided: true,
        })
        .collect();
    let mut manifest=ExportManifest {
        package_version:1,geometry_version:53,material_version:20,bake_version:22,
        prototype_hash:prototype.hash.clone(),coordinate_system:"right-handed, native Z-up, metres; OBJ UV V points up".into(),
        normal_convention:"OpenGL tangent-space +Y; linear normal maps".into(),mask_channels:"RGBA = occlusion, roughness, metallic, 1; linear".into(),
        door_policy:"LOD2 appearance uses closed leaves; preserve instance angles independently. door-N.obj and door-N-hardware.obj share hinge-local coordinates and map to doors.json[N]. depth_offset is the signed hinge-local Y centre for leaf collision; exported leaf and hardware vertices already include it. Missing depth_offset defaults to zero. New doors default closed; removed IDs are ignored.".into(),
        terrain_policy:"Floor top is local Z=0. Place a level building so the highest terrain point within its foundation footprint touches Z=0; terrain gradients must not exceed 30 degrees. Foundation depth includes burial margin. Stair pitch is steeper than 30 degrees and its toe reaches the worst-case downhill ground beyond the footprint. Hide the stairs and stair_treads material groups together at every LOD; disable stairs-collision.json with them. collision.json always includes the foundation; stairs.json describes each entrance, flight direction and landing bounds; foundation.json gives the buried bounds. Baked foundation maps are shared by stair bodies at all LODs; stair_treads use stone_slab maps, also used by the LOD0 fireplace hearth. Preserve tread overhang collision from stairs-collision.json with stair visibility.".into(),
        floor_policy:"Ground floor is Z=0; storey N is N*(wall_height+floor_thickness). Opening sills and partition dimensions are floor-relative. layout.json contains every floor height and the resolved internal staircase, including floor and turn landings. support-collision.json contains occupied floors and internal treads/landings. These supports and collision.json remain active at every LOD, independently of hideable exterior entrance stairs. Enclosed stairwells use interior wall surfaces at LOD0, with one outward-opening stairwell-door-N on each floor and exterior=false in doors.json. Internal doors use the shared door material and dimensions, retain independent angles and are shown at LOD0 only. Stairwell bounds, doorway approach points and enclosure metadata are in layout.json. enclosure.shared_sides is ordered near/right/far/left in stair-local coordinates; shared sides reuse external walls and remaining partitions extend to them. enclosure.merge_unit is the authored wall U module in metres. Staircase reversed rotates the across/along basis 180 degrees without changing its axis. Internal stair meshes use timber at LOD0 only. Landing perimeter guards are omitted within 0.6 m of an external wall including inward timber projection; flight rails remain. Floor openings have timber trimmers and headers beneath the boards, inset 0.015 m from adjacent wall planes, with a shallower landing-edge header for headroom. Each occupied floor has a fireplace and hearth aligned beneath one shared roof chimney and cap; stair placement reserves that wall and the hearth approach for the rooms. Automatic stairs rank both axes by proximity to usable exterior walls with a 0.04 m timber-face clearance; entrance offsets are preferences and move along their facade to clear the selected stairs, including their leaf swing and player approach. Generated ground-floor windows and external entrance steps follow the resolved door positions; authored manual windows remain fixed. Upper floors and joists have real staircase cutouts; only the top storey carries the roof.".into(),
        masked_materials,lod_triangles:std::array::from_fn(|index|prototype.lods[index].triangle_count()),files,
    };
    manifest.door_policy.push_str(" Optional collision_sections contains hinge-local solid boxes for shaped leaves. Rotate every section with the leaf; missing/empty sections use the rectangular width/height/thickness bounds. Each Gothic church entrance exports separate -left and -right leaf IDs, hinges, meshes, hardware and saved angles. Its timber leaves fill the complete pointed arch; the recipe door height is the spring line, where the arch begins.");
    if prototype.recipe.church.is_some() {
        manifest.floor_policy="One connected cruciform church at Z=0, without occupied upper storeys or furnishings. layout.church describes the nave, transept, towers and counter-clockwise floor outline. Floors/support and buried foundations follow that outline; foundation.json is their enclosing bounds. LOD0 has openable entrance joinery, pointed see-through windows, tracery and interior stone ribs. LOD1 has opaque glazing and no interior surfaces. LOD2 uses padded, baked stone, roof, portal and pointed-window bay charts. Twin towers and open flying-buttress arches retain their silhouette at every LOD. Preserve instance door angles, floor support and collisions at every LOD.".into();
    }
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::write(root.join("manifest.json"), bytes)?;
    fs::File::open(root.join("manifest.json"))?.sync_all()?;
    fs::File::open(root)?.sync_all()?;
    Ok(manifest)
}

fn write_grub_filter(
    root: &Path,
    recipe: &BuildingRecipe,
    files: &mut Vec<ExportFile>,
) -> Result<(), ExportError> {
    let allowed_materials: Vec<_> = [
        MaterialSlot::Foundation,
        MaterialSlot::Stairs,
        MaterialSlot::StairTreads,
        MaterialSlot::Wall,
        MaterialSlot::Roof,
        MaterialSlot::ThatchEnd,
        MaterialSlot::ThatchShell1,
        MaterialSlot::ThatchShell2,
        MaterialSlot::ThatchShell3,
        MaterialSlot::ThatchShell4,
    ]
    .into_iter()
    .filter(|slot| crate::grub::exterior_material(recipe, *slot))
    .map(material_name)
    .collect();
    let descriptor = serde_json::json!({
        "settings": recipe.grub_filter,
        "uniform_vectors": recipe.grub_filter.uniform_vectors(recipe.seed),
        "exterior_only": true,
        "allowed_materials": allowed_materials,
        "surface_rule": "Apply only to allowed_materials. All unlisted or shared groups stay clean. Wall is included for wattle_and_daub and dedicated church exterior faces, roof/thatch groups only for thatch. The mixed LOD2 facade atlas stays unfiltered to protect its baked timber and joinery.",
        "coordinate_system": "Building-local Y-up metres: native (x,y,z) becomes (x,z,-y). Floor height is zero. Moving leaves carry absolute building height separately from their leaf-local noise coordinates.",
        "height_profile": "Building-floor-relative: influence = 1 / (1 + exp2(height / height_falloff)); strength = clamp(2 * influence * combined_noise, 0, 1). Neutral noise (0.5) gives 50% at the building floor; terrain height is never used.",
        "noise_layers": "Contrast broad and fine 3D noise separately, then mix using fine_mix. patch_size and fine_patch_size are building-local metres.",
        "normal_rule": "Dot the final normal-mapped world normal with transformed building up; smoothstep(0.35,0.85,upness) favours upward faces.",
        "protected_materials": ["floor", "ceiling", "interior", "timber", "trim", "door", "chimney", "stone_slab", "firebox", "glass", "hardware", "window_proxy", "atlas"],
        "base_maps": "Unfiltered. Apply grub-filter.wgsl before PBR lighting only to allowed groups; preserve the original alpha and normal.",
        "shader": "grub-filter.wgsl"
    });
    write_json(root, "grub-filter.json", &descriptor, files)?;
    let source = crate::grub::SHADER_SOURCE
        .strip_prefix("#define_import_path island_building::grub\n")
        .unwrap_or(crate::grub::SHADER_SOURCE);
    write_bytes(root, "grub-filter.wgsl", source.as_bytes(), files)
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
            "timber",
            MaterialSlot::Timber,
            &mut recipe.materials.timber_recipe,
        ),
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
        (
            "chimney",
            MaterialSlot::Chimney,
            &mut recipe.materials.chimney_recipe,
        ),
        (
            "floor",
            MaterialSlot::Floor,
            &mut recipe.materials.floor_recipe,
        ),
        (
            "foundation",
            MaterialSlot::Foundation,
            &mut recipe.materials.foundation_recipe,
        ),
        (
            "stone-slab",
            MaterialSlot::StoneSlab,
            &mut recipe.materials.stone_slab_recipe,
        ),
        (
            "ceiling",
            MaterialSlot::Ceiling,
            &mut recipe.materials.ceiling_recipe,
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
        if texture.slot.thatch_shell_layer().is_some() {
            let opacity: Vec<_> = texture
                .albedo
                .chunks_exact(4)
                .flat_map(|pixel| [pixel[3], pixel[3], pixel[3], 255])
                .collect();
            write_map(
                root,
                &format!("maps/{name}-opacity.png"),
                texture.size,
                &opacity,
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
            append_mesh(
                &mut obj,
                &part.mesh,
                &mut vertex_base,
                uv_scale(prototype, part.material),
            );
        }
        write_bytes(root, &format!("lod{index}.obj"), obj.as_bytes(), files)?;
    }
    for (index, door) in prototype.doors.iter().enumerate() {
        let mut obj = String::from(
            "# Door leaf in hinge-local coordinates; use doors.json for hinge and angles\nmtllib building.mtl\nusemtl door\n",
        );
        append_mesh(
            &mut obj,
            &door.mesh,
            &mut 1,
            uv_scale(prototype, MaterialSlot::Door),
        );
        write_bytes(root, &format!("door-{index}.obj"), obj.as_bytes(), files)?;
        let mut hardware = String::from(
            "# Door hardware shares the leaf hinge-local coordinates and animation\nmtllib building.mtl\nusemtl hardware\n",
        );
        append_mesh(&mut hardware, &door.hardware_mesh, &mut 1, [1.; 2]);
        write_bytes(
            root,
            &format!("door-{index}-hardware.obj"),
            hardware.as_bytes(),
            files,
        )?;
    }
    Ok(())
}

fn uv_scale(prototype: &BuildingPrototype, slot: MaterialSlot) -> [f32; 2] {
    let slot = slot.texture_slot();
    prototype
        .materials
        .iter()
        .find(|texture| texture.slot == slot)
        .map_or([1.; 2], |texture| {
            texture.physical_size.map(|size| 2. / size.max(0.001))
        })
}

fn append_mesh(output: &mut String, mesh: &BuildingMesh, base: &mut usize, scale: [f32; 2]) {
    for [x, y, z] in &mesh.positions {
        writeln!(output, "v {x} {y} {z}").unwrap();
    }
    for [u, v] in &mesh.uvs {
        writeln!(output, "vt {} {}", u * scale[0], 1. - v * scale[1]).unwrap();
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
        MaterialSlot::Timber => "timber",
        MaterialSlot::Roof => "roof",
        MaterialSlot::ThatchEnd => "thatch_end",
        MaterialSlot::ThatchShell1 => "thatch_shell_1",
        MaterialSlot::ThatchShell2 => "thatch_shell_2",
        MaterialSlot::ThatchShell3 => "thatch_shell_3",
        MaterialSlot::ThatchShell4 => "thatch_shell_4",
        MaterialSlot::Floor => "floor",
        MaterialSlot::Foundation => "foundation",
        MaterialSlot::Stairs => "stairs",
        MaterialSlot::StoneSlab => "stone_slab",
        MaterialSlot::StairTreads => "stair_treads",
        MaterialSlot::Ceiling => "ceiling",
        MaterialSlot::Interior => "interior",
        MaterialSlot::Trim => "trim",
        MaterialSlot::Door => "door",
        MaterialSlot::Hardware => "hardware",
        MaterialSlot::Chimney => "chimney",
        MaterialSlot::Firebox => "firebox",
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
        (MaterialSlot::Timber, p.timber),
        (MaterialSlot::Roof, p.roof),
        (MaterialSlot::Floor, p.floor),
        (MaterialSlot::Foundation, p.foundation),
        (MaterialSlot::Stairs, p.foundation),
        (MaterialSlot::StoneSlab, p.stone_slab),
        (MaterialSlot::StairTreads, p.stone_slab),
        (MaterialSlot::Ceiling, p.ceiling),
        (MaterialSlot::Interior, p.interior),
        (MaterialSlot::Trim, p.trim),
        (MaterialSlot::Door, p.door),
        (MaterialSlot::Hardware, [0.48, 0.44, 0.34]),
        (MaterialSlot::Chimney, p.chimney),
        (MaterialSlot::Firebox, [0.018, 0.014, 0.012]),
        (MaterialSlot::Glass, [p.glass[0], p.glass[1], p.glass[2]]),
        (MaterialSlot::WindowProxy, [0.16, 0.23, 0.25]),
        (MaterialSlot::Atlas, [1.; 3]),
    ]
    .into_iter()
    .chain(
        std::iter::once(MaterialSlot::ThatchEnd)
            .chain(MaterialSlot::THATCH_SHELLS)
            .filter(|slot| {
                prototype
                    .materials
                    .iter()
                    .any(|texture| texture.slot == *slot)
            })
            .map(|slot| (slot, [1.; 3])),
    ) {
        let name = material_name(slot);
        let map_slot = slot.texture_slot();
        let map_name = material_name(map_slot);
        let textured = slot == MaterialSlot::Atlas
            || prototype
                .materials
                .iter()
                .any(|texture| texture.slot == map_slot);
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
                "map_Kd maps/{map_name}-albedo.png\nmap_Bump maps/{map_name}-normal.png"
            )
            .unwrap();
        }
        if slot.thatch_shell_layer().is_some() {
            writeln!(
                text,
                "map_d maps/{name}-opacity.png\n# alpha_cutoff 0.5; double_sided true"
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
    fn thatch_package_preserves_masked_shells_physical_uvs_and_regeneration() {
        let target = directory("thatch-shells");
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.roof.covering = crate::RoofCovering::Thatch;
        let prototype = crate::generate(&recipe).unwrap();
        let manifest = export_package(&prototype, &target, Path::new(".")).unwrap();
        assert_eq!(manifest.masked_materials.len(), 4);
        let mtl = fs::read_to_string(target.join("building.mtl")).unwrap();
        assert!(mtl.contains("newmtl thatch_end\n"));
        assert!(mtl.contains("map_Kd maps/thatch_end-albedo.png"));
        assert!(
            manifest
                .files
                .iter()
                .any(|file| file.path == "maps/thatch_end-normal.png")
        );
        for feature in &manifest.masked_materials {
            assert!(feature.double_sided);
            assert!((feature.alpha_cutoff - 0.5).abs() < f32::EPSILON);
            let opacity = format!("maps/{}-opacity.png", feature.name);
            assert!(mtl.contains(&format!("map_d {opacity}")));
            assert!(manifest.files.iter().any(|file| file.path == opacity));
        }
        let obj = fs::read_to_string(target.join("lod0.obj")).unwrap();
        let after_material = obj.split("usemtl thatch_shell_1\n").nth(1).unwrap();
        let uv: Vec<f32> = after_material
            .lines()
            .find(|line| line.starts_with("vt "))
            .unwrap()
            .split_whitespace()
            .skip(1)
            .map(|value| value.parse().unwrap())
            .collect();
        let source = prototype.lods[0]
            .parts
            .iter()
            .find(|part| part.material == MaterialSlot::ThatchShell1)
            .unwrap()
            .mesh
            .uvs[0];
        assert!((uv[0] - source[0] * 12.5).abs() < 0.00001);
        assert!((uv[1] - (1. - source[1] * 12.5)).abs() < 0.00001);
        let loaded: BuildingRecipe =
            serde_json::from_slice(&fs::read(target.join("recipe.json")).unwrap()).unwrap();
        let regenerated = crate::generate_with_base(&loaded, &target).unwrap();
        assert_eq!(prototype.hash, regenerated.hash);
        assert_eq!(prototype.materials, regenerated.materials);
        fs::remove_dir_all(target).unwrap();
    }

    fn write_package_test_materials(source: &Path) {
        for (name, bytes) in [
            (
                "wall",
                include_bytes!("../../island-rs/texture-recipes/cracked-stone.json").as_slice(),
            ),
            (
                "roof",
                include_bytes!("../../island-rs/texture-recipes/slate-roof.json").as_slice(),
            ),
            (
                "door",
                include_bytes!("../../island-rs/texture-recipes/wooden-planks.json").as_slice(),
            ),
            (
                "chimney",
                include_bytes!("../../island-rs/texture-recipes/rough-stone.json").as_slice(),
            ),
            (
                "timber",
                include_bytes!("../../island-rs/texture-recipes/wooden-planks.json").as_slice(),
            ),
            (
                "floor",
                include_bytes!("../../island-rs/texture-recipes/wooden-floorboards.json")
                    .as_slice(),
            ),
            (
                "ceiling",
                include_bytes!("../../island-rs/texture-recipes/wooden-floorboards.json")
                    .as_slice(),
            ),
        ] {
            fs::write(source.join(format!("{name}.json")), bytes).unwrap();
        }
    }

    #[test]
    fn package_is_portable_hashed_and_preserves_doors() {
        let target = directory("complete");
        let source = directory("source");
        fs::create_dir(&source).unwrap();
        write_package_test_materials(&source);
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.roof.covering = crate::RoofCovering::Slate;
        recipe.walls.style = crate::WallStyle::WattleAndDaub;
        recipe.openings[0].door_style = crate::DoorStyle::FrameAndPanel;
        recipe.foundation.stair_run = crate::StairRun::AlongWall;
        recipe.materials.wall_recipe = Some("wall.json".into());
        recipe.materials.roof_recipe = Some("roof.json".into());
        recipe.materials.door_recipe = Some("door.json".into());
        recipe.materials.chimney_recipe = Some("chimney.json".into());
        recipe.materials.timber_recipe = Some("timber.json".into());
        recipe.materials.floor_recipe = Some("floor.json".into());
        recipe.materials.ceiling_recipe = Some("ceiling.json".into());
        recipe.materials.foundation_recipe = Some("chimney.json".into());
        recipe.materials.stone_slab_recipe = Some("chimney.json".into());
        recipe.grub_filter.enabled = true;
        let prototype = crate::generate_with_base(&recipe, &source).unwrap();
        let manifest = export_package(&prototype, &target, &source).unwrap();
        fs::remove_dir_all(&source).unwrap();
        for entry in &manifest.files {
            let bytes = fs::read(target.join(&entry.path)).unwrap();
            assert_eq!(entry.sha256, format!("{:x}", Sha256::digest(&bytes)));
        }
        let loaded: crate::BuildingRecipe =
            serde_json::from_slice(&fs::read(target.join("recipe.json")).unwrap()).unwrap();
        let regenerated = crate::generate_with_base(&loaded, &target).unwrap();
        assert_eq!(regenerated.hash, prototype.hash);
        assert_eq!(regenerated.atlas, prototype.atlas);
        assert_eq!(regenerated.doors, prototype.doors);
        assert_eq!(regenerated.lods, prototype.lods);
        assert_eq!(regenerated.stair_colliders, prototype.stair_colliders);
        assert_eq!(regenerated.materials, prototype.materials);
        for surface in ["floor", "ceiling", "foundation"] {
            assert!(target.join(format!("dependencies/{surface}.json")).exists());
            assert!(target.join(format!("maps/{surface}-albedo.png")).exists());
        }
        assert_eq!(
            loaded.openings[0].door_style,
            crate::DoorStyle::FrameAndPanel
        );
        let doors: Vec<crate::DoorDescriptor> =
            serde_json::from_slice(&fs::read(target.join("doors.json")).unwrap()).unwrap();
        assert_eq!(doors, prototype.doors);
        assert!(target.join("lod2.obj").exists());
        assert!(target.join("door-0-hardware.obj").exists());
        assert!(target.join("dependencies/door.json").exists());
        assert_eq!(
            loaded.materials.timber_recipe.as_deref(),
            Some("dependencies/timber.json")
        );
        assert!(target.join("maps/timber-albedo.png").exists());
        assert_eq!(loaded.roof.covering, crate::RoofCovering::Slate);
        assert_eq!(
            loaded.materials.roof_recipe.as_deref(),
            Some("dependencies/roof.json")
        );
        assert!(target.join("dependencies/roof.json").exists());
        assert_eq!(
            (
                manifest.geometry_version,
                manifest.material_version,
                manifest.bake_version
            ),
            (53, 20, 22)
        );
        assert_eq!(
            loaded.materials.door_recipe.as_deref(),
            Some("dependencies/door.json")
        );
        assert!(target.join("dependencies/chimney.json").exists());
        assert_slab_dependency(&target, &loaded);
        assert_grub_export(&target);
        assert_eq!(
            loaded.materials.chimney_recipe.as_deref(),
            Some("dependencies/chimney.json")
        );
        assert!(
            manifest
                .files
                .iter()
                .any(|file| file.path == "maps/chimney-albedo.png")
        );
        let portable: crate::BuildingPrototype =
            serde_json::from_slice(&fs::read(target.join("prototype.json")).unwrap()).unwrap();
        assert_eq!(
            portable.recipe.materials.chimney_recipe,
            loaded.materials.chimney_recipe
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

    fn assert_grub_export(target: &Path) {
        let descriptor: serde_json::Value =
            serde_json::from_slice(&fs::read(target.join("grub-filter.json")).unwrap()).unwrap();
        assert_eq!(descriptor["settings"]["enabled"], true);
        assert_eq!(descriptor["exterior_only"], true);
        assert_eq!(descriptor["uniform_vectors"].as_array().unwrap().len(), 5);
        assert!(
            descriptor["protected_materials"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("floor"))
        );
        assert!(
            fs::read_to_string(target.join("grub-filter.wgsl"))
                .unwrap()
                .contains("fn grub_weights")
        );
    }

    fn assert_slab_dependency(target: &Path, loaded: &BuildingRecipe) {
        assert!(target.join("dependencies/stone-slab.json").exists());
        assert_eq!(
            loaded.materials.stone_slab_recipe.as_deref(),
            Some("dependencies/stone-slab.json")
        );
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

    #[test]
    fn changed_chimney_dependency_is_rejected_without_publishing_a_package() {
        let target = directory("changed-chimney-package");
        let source = directory("changed-chimney-source");
        fs::create_dir(&source).unwrap();
        let material = source.join("stone.json");
        let mut bytes = include_bytes!("../../island-rs/texture-recipes/rough-stone.json").to_vec();
        fs::write(&material, &bytes).unwrap();
        let mut recipe = crate::BuildingRecipe::hut();
        recipe.atlas_size = 128;
        recipe.materials.chimney_recipe = Some("stone.json".into());
        let prototype = crate::generate_with_base(&recipe, &source).unwrap();
        // Even a semantically equivalent external edit must not be attributed
        // to the source hash captured by the already generated prototype.
        bytes.push(b'\n');
        fs::write(&material, bytes).unwrap();
        let error = export_package(&prototype, &target, &source).unwrap_err();
        assert!(error.to_string().contains("changed since generation"));
        assert!(!target.exists());
        fs::remove_dir_all(source).unwrap();
    }
}
