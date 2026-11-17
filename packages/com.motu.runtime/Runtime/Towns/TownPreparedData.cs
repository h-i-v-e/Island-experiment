using System;
using UnityEngine;

namespace Motu.Towns
{
    [Serializable] internal sealed class TownPreparedData
    {
        public int schema_version;
        public TownPrototype[] prototypes = Array.Empty<TownPrototype>();
        public TownInstance[] instances = Array.Empty<TownInstance>();
        public TownMesh supports;
        public TownBox[] support_colliders, stair_colliders;
        public TownMaterial support_material;
        public string clearance_json;
        public string roads_json;
        public TownMesh road_mesh;
        public int road_count;
        public TownBridgeCandidate[] bridge_candidates;
        public string[] diagnostics;
        public float[] mouth;
        public string seed;
    }
    [Serializable] internal sealed class TownPrototype
    {
        public string entry_id, hash;
        public TownLod[] lods;
        public TownDoorData[] doors;
        public TownBox[] colliders, support_colliders;
        public TownMaterial[] materials;
        public TownMaterial atlas;
    }
    [Serializable] internal sealed class TownLod { public TownPart[] parts; }
    [Serializable] internal sealed class TownPart { public string material; public TownMesh mesh; }
    [Serializable] internal sealed class TownMesh
    {
        public float[] positions, normals, uvs;
        public float[] grub_positions;
        public int[] indices;
    }
    [Serializable] internal sealed class TownMaterial
    {
        public string slot, albedo_png, normal_png, masks_png;
        public int size;
        public float[] physical_size;
        public float[] colour;
        public float[] grub_uniform;
    }
    [Serializable] internal sealed class TownInstance
    {
        public int prototype_index;
        public float[] position;
        public float yaw_radians;
        public TownFrontage frontage;
    }
    [Serializable] internal sealed class TownFrontage
    {
        public string door_id;
        public float[] street_edge;
    }
    [Serializable] internal sealed class TownBridgeCandidate
    {
        public float[] from, to;
        public float width;
    }
    [Serializable] internal sealed class TownBox { public float[] min, max; }
    [Serializable] internal sealed class TownDoorData
    {
        public string id;
        public float[] hinge;
        public float closed_angle, swing_angle, width, height, thickness, depth_offset;
        public bool exterior;
        public TownBox[] collision_sections;
        public TownMesh mesh, hardware_mesh;
    }
}
