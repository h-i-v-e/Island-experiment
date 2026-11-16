use super::{CompiledBuildingPart, CompiledBuildingPrototype, native_to_bevy};
use crate::interaction::{BuildingInstance, BuildingLodRoot, BuildingStairs, DoorLeaf};
use bevy::{
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use std::sync::Arc;

/// Spawns one placement with retained independent doors and shared asset handles.
pub fn spawn_building(
    commands: &mut Commands,
    prototype: Arc<CompiledBuildingPrototype>,
    placement_id: impl Into<String>,
    transform: Transform,
) -> Entity {
    let root = commands
        .spawn((Name::new("Building"), transform, Visibility::Inherited))
        .id();
    let mut near_root = root;
    for (level, parts) in prototype.lods.iter().enumerate() {
        let lod_root = commands
            .spawn((
                Name::new(format!("Building LOD{level}")),
                Transform::default(),
                if level == 0 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                BuildingLodRoot {
                    building: root,
                    level,
                },
                ChildOf(root),
            ))
            .id();
        if level == 0 {
            near_root = lod_root;
        }
        for part in parts {
            let entity = spawn_part(commands, part, Transform::default(), lod_root);
            if part.stairs {
                commands
                    .entity(entity)
                    .insert(BuildingStairs { building: root });
            }
        }
    }
    for (index, (door, part)) in prototype
        .source
        .doors
        .iter()
        .zip(&prototype.doors)
        .enumerate()
    {
        let leaf = spawn_part(
            commands,
            part,
            Transform {
                translation: native_to_bevy(door.hinge),
                rotation: Quat::from_rotation_y(door.closed_angle),
                ..default()
            },
            if door.exterior { root } else { near_root },
        );
        if let Some(hardware) = prototype
            .door_hardware
            .get(index)
            .filter(|part| part.triangles > 0)
        {
            let knob = spawn_part(commands, hardware, Transform::default(), leaf);
            commands
                .entity(knob)
                .insert(Name::new(format!("Door {} knobs", door.id)));
        }
        commands.entity(leaf).insert((
            Name::new(format!("Door {}", door.id)),
            DoorLeaf {
                building: root,
                door_id: door.id.clone(),
            },
        ));
    }
    commands
        .entity(root)
        .insert(BuildingInstance::new(prototype, placement_id.into()));
    root
}

fn spawn_part(
    commands: &mut Commands,
    part: &CompiledBuildingPart,
    transform: Transform,
    parent: Entity,
) -> Entity {
    let mut entity = commands.spawn((
        Mesh3d(part.mesh.clone()),
        transform,
        Visibility::Inherited,
        ChildOf(parent),
    ));
    if let Some(material) = &part.array_material {
        entity.insert(MeshMaterial3d(material.clone()));
    } else if let Some(material) = &part.grub_material {
        entity.insert(MeshMaterial3d(material.clone()));
    } else {
        entity.insert(MeshMaterial3d(part.material.clone()));
    }
    if part.glass {
        entity.insert((NotShadowCaster, NotShadowReceiver));
    }
    entity.id()
}
