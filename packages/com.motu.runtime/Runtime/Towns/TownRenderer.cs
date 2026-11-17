using System;
using System.Collections.Generic;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Towns
{
    /// <summary>Imports native results only. Placement and terrain fitting live in Rust.</summary>
    public sealed class TownRenderer : MonoBehaviour
    {
        private readonly List<UnityEngine.Object> owned = new List<UnityEngine.Object>();
        private MeshRenderer roads;
        private TownBridgeCandidate[] bridgeCandidates = Array.Empty<TownBridgeCandidate>();
        private const float NativeWorldSizeMetres = 2000f;
        internal GameObject[] SharedPrototypes { get; private set; } = Array.Empty<GameObject>();
        public int BuildingCount { get; private set; }
        public int RoadCount { get; private set; }
        public int RoadFrontageCount { get; private set; }
        public int BridgeCandidateCount => bridgeCandidates.Length;
        public Vector3 MouthLocalPosition { get; private set; }
        public Vector3 InspectionLocalPosition { get; private set; }

        internal static TownRenderer Install(Transform parent, TownPreparedData data, float worldSize,
            bool includeUnplacedPrototypes = false, Material roadMaterial = null,
            TerrainMaterialTextureArrays terrainTextures = null)
        {
            if (data == null || (!includeUnplacedPrototypes && (data.instances?.Length ?? 0) == 0
                && (data.road_mesh?.indices?.Length ?? 0) == 0 && (data.bridge_candidates?.Length ?? 0) == 0)) return null;
            var root = new GameObject("River-mouth town");
            root.transform.SetParent(parent, false);
            root.transform.localScale = Vector3.one * (worldSize / NativeWorldSizeMetres);
            var renderer = root.AddComponent<TownRenderer>();
            try { renderer.Build(data, roadMaterial, terrainTextures); return renderer; }
            catch { DestroyUnityObject(root); throw; }
        }

        private void Build(TownPreparedData data, Material roadMaterial, TerrainMaterialTextureArrays terrainTextures)
        {
            const float worldSize = NativeWorldSizeMetres;
            var templatesRoot = new GameObject("Shared building prototypes");
            templatesRoot.transform.SetParent(transform, false);
            templatesRoot.SetActive(false);
            var prototypes = new GameObject[data.prototypes.Length];
            SharedPrototypes = prototypes;
            var instances = data.instances ?? Array.Empty<TownInstance>();
            InspectionLocalPosition = instances.Length > 0 ? Position(instances[0].position, worldSize) : Vector3.zero;
            for (var index = 0; index < prototypes.Length; index++)
                prototypes[index] = BuildPrototype(templatesRoot.transform, data.prototypes[index]);
            foreach (var instance in instances)
            {
                if (instance.prototype_index < 0 || instance.prototype_index >= prototypes.Length)
                    throw new InvalidOperationException("The native town refers to an invalid building prototype.");
                var building = Instantiate(prototypes[instance.prototype_index], transform, false);
                building.name = data.prototypes[instance.prototype_index].entry_id;
                building.transform.localPosition = Position(instance.position, worldSize);
                building.transform.localRotation = Quaternion.Euler(0f, -instance.yaw_radians * Mathf.Rad2Deg, 0f);
                building.SetActive(true);
                BuildingCount++;
                if (instance.frontage?.street_edge?.Length == 3)
                {
                    if (RoadFrontageCount == 0) InspectionLocalPosition = Position(instance.frontage.street_edge, worldSize);
                    RoadFrontageCount++;
                }
            }
            if (data.supports?.indices != null && data.supports.indices.Length > 0)
            {
                var mesh = CreateMesh(data.supports, "Combined terrain-fitted foundations and stairs");
                var supportMaterial = CreateMaterial(data.support_material, "foundation");
                if (data.supports.grub_positions != null && data.supports.grub_positions.Length == mesh.vertexCount * 3)
                {
                    var local = new List<Vector3>(mesh.vertexCount);
                    for (var index = 0; index < mesh.vertexCount; index++)
                        local.Add(Vector(data.supports.grub_positions, index * 3));
                    mesh.SetUVs(1, local);
                    supportMaterial.SetFloat("_UseGrubCoordinates", 1f);
                }
                var support = AddMesh(transform, mesh, supportMaterial, mesh.name);
                support.transform.localPosition = new Vector3(-worldSize * 0.5f, 0f, -worldSize * 0.5f);
                support.AddComponent<MeshCollider>().sharedMesh = mesh;
            }
            if (data.mouth != null && data.mouth.Length >= 2)
                MouthLocalPosition = new Vector3(data.mouth[0] - worldSize * 0.5f, 0f, data.mouth[1] - worldSize * 0.5f);
            RoadCount = data.road_count;
            bridgeCandidates = data.bridge_candidates ?? Array.Empty<TownBridgeCandidate>();
            if (data.road_mesh?.indices?.Length > 0)
                BuildRoads(data.road_mesh, roadMaterial, terrainTextures);
            else if (instances.Length == 0) InspectionLocalPosition = MouthLocalPosition;
        }

        private void BuildRoads(TownMesh data, Material material, TerrainMaterialTextureArrays terrainTextures)
        {
            var mesh = CreateMesh(data, "Town roads");
            if (material == null)
            {
                var shader = Resources.Load<Shader>("Towns/TownRoad");
                if (shader == null) throw new InvalidOperationException("The Motu town road shader is missing.");
                material = Own(new Material(shader) { name = "Town road surface" });
                terrainTextures?.BindRoad(material);
            }
            var root = AddMesh(transform, mesh, material, mesh.name);
            // Give the surface dressing 4 cm of clearance over the independently
            // triangulated terrain. Player collision still uses the native grade.
            root.transform.localPosition = new Vector3(-NativeWorldSizeMetres * 0.5f, 0.04f, -NativeWorldSizeMetres * 0.5f);
            roads = root.GetComponent<MeshRenderer>();
            roads.shadowCastingMode = ShadowCastingMode.Off;
            if (BuildingCount == 0) InspectionLocalPosition = mesh.bounds.center + root.transform.localPosition;
        }

        internal void SetRoadVisibility(bool visible)
        {
            if (roads != null) roads.enabled = visible;
        }

        public bool TryGetBridgeCandidate(int index, out Vector3 from, out Vector3 to, out float width)
        {
            from = to = default;
            width = 0f;
            if ((uint)index >= (uint)bridgeCandidates.Length) return false;
            var candidate = bridgeCandidates[index];
            if (candidate?.from?.Length != 3 || candidate.to?.Length != 3) return false;
            from = transform.TransformPoint(Position(candidate.from, NativeWorldSizeMetres));
            to = transform.TransformPoint(Position(candidate.to, NativeWorldSizeMetres));
            width = transform.TransformVector(Vector3.right * candidate.width).magnitude;
            return true;
        }

        private GameObject BuildPrototype(Transform parent, TownPrototype source)
        {
            var root = new GameObject(source.entry_id + " [" + source.hash + "]");
            root.transform.SetParent(parent, false);
            var materials = new Dictionary<string, Material>(StringComparer.Ordinal);
            foreach (var material in source.materials ?? Array.Empty<TownMaterial>())
            {
                var imported = CreateMaterial(material, material.slot);
                imported.name = source.entry_id + " " + material.slot;
                materials[material.slot] = imported;
            }
            var atlas = CreateMaterial(source.atlas, "atlas");
            atlas.name = source.entry_id + " atlas";
            Material Resolve(string slot)
            {
                if (materials.TryGetValue(slot, out var material)) return material;
                if ((slot == "stairs" || slot == "stair_treads") && materials.TryGetValue("foundation", out material)) return material;
                return atlas;
            }
            var lods = new LOD[source.lods.Length];
            for (var level = 0; level < source.lods.Length; level++)
            {
                var renderers = new List<Renderer>();
                foreach (var part in source.lods[level].parts)
                {
                    var mesh = CreateMesh(part.mesh, source.entry_id + " LOD" + level + " " + part.material);
                    renderers.Add(AddMesh(root.transform, mesh, Resolve(part.material), mesh.name).GetComponent<Renderer>());
                }
                lods[level] = new LOD(level == 0 ? 0.3f : level == 1 ? 0.08f : 0.008f, renderers.ToArray());
            }
            AddBoxes(root, source.colliders);
            AddBoxes(root, source.support_colliders);
            foreach (var door in source.doors ?? Array.Empty<TownDoorData>())
                AddDoor(root.transform, door, Resolve("door"), Resolve("hardware"));
            var doors = new List<Renderer>();
            foreach (var door in root.GetComponentsInChildren<TownDoor>())
                doors.AddRange(door.GetComponentsInChildren<Renderer>());
            // Native coarse LODs already contain their closed-door appearance.
            var nearRenderers = new List<Renderer>(lods[0].renderers);
            nearRenderers.AddRange(doors);
            lods[0].renderers = nearRenderers.ToArray();
            var group = root.AddComponent<LODGroup>();
            group.SetLODs(lods);
            group.RecalculateBounds();
            return root;
        }

        private void AddDoor(Transform parent, TownDoorData data, Material leafMaterial, Material hardwareMaterial)
        {
            var hinge = new GameObject("Door " + data.id);
            hinge.transform.SetParent(parent, false);
            hinge.transform.localPosition = Vector(data.hinge);
            var door = hinge.AddComponent<TownDoor>();
            door.Initialize(data);
            AddMesh(hinge.transform, CreateMesh(data.mesh, data.id + " leaf"), leafMaterial, "Leaf");
            if (data.hardware_mesh?.indices != null && data.hardware_mesh.indices.Length > 0)
                AddMesh(hinge.transform, CreateMesh(data.hardware_mesh, data.id + " hardware"), hardwareMaterial, "Hardware");
            if (data.collision_sections != null && data.collision_sections.Length > 0) AddBoxes(hinge, data.collision_sections);
            else
            {
                var collider = hinge.AddComponent<BoxCollider>();
                collider.center = new Vector3(data.width * 0.5f, data.height * 0.5f, data.depth_offset);
                collider.size = new Vector3(data.width, data.height, data.thickness);
            }
        }

        private static void AddBoxes(GameObject owner, TownBox[] boxes)
        {
            foreach (var box in boxes ?? Array.Empty<TownBox>())
            {
                var min = Vector(box.min);
                var max = Vector(box.max);
                var collider = owner.AddComponent<BoxCollider>();
                collider.center = (min + max) * 0.5f;
                collider.size = max - min;
            }
        }

        private static GameObject AddMesh(Transform parent, Mesh mesh, Material material, string name)
        {
            var child = new GameObject(name);
            child.transform.SetParent(parent, false);
            child.AddComponent<MeshFilter>().sharedMesh = mesh;
            child.AddComponent<MeshRenderer>().sharedMaterial = material;
            return child;
        }

        internal Mesh CreateMesh(TownMesh source, string name)
        {
            if (source == null || source.positions == null || source.positions.Length % 3 != 0)
                throw new InvalidOperationException("The native town returned invalid mesh positions.");
            var count = source.positions.Length / 3;
            var positions = new Vector3[count];
            var normals = new Vector3[count];
            var uv = new Vector2[count];
            for (var i = 0; i < count; i++)
            {
                positions[i] = Vector(source.positions, i * 3);
                normals[i] = Vector(source.normals, i * 3);
                uv[i] = new Vector2(source.uvs[i * 2], source.uvs[i * 2 + 1]);
            }
            var triangles = (int[])source.indices.Clone();
            // Swapping native Y/Z changes handedness.
            for (var i = 0; i < triangles.Length; i += 3)
                (triangles[i + 1], triangles[i + 2]) = (triangles[i + 2], triangles[i + 1]);
            var mesh = Own(new Mesh { name = name, indexFormat = count > 65535 ? IndexFormat.UInt32 : IndexFormat.UInt16 });
            mesh.vertices = positions;
            mesh.normals = normals;
            mesh.uv = uv;
            mesh.triangles = triangles;
            mesh.RecalculateBounds();
            return mesh;
        }

        private Material CreateMaterial(TownMaterial source, string slot)
        {
            var shader = Resources.Load<Shader>("Towns/TownBuilding");
            if (shader == null) throw new InvalidOperationException("The Motu town building shader is missing.");
            var material = Own(new Material(shader) { name = "Town " + slot, enableInstancing = true });
            if (source != null)
            {
                if (source.colour != null && source.colour.Length == 4)
                    material.SetColor("_BaseColor", new Color(source.colour[0], source.colour[1], source.colour[2], source.colour[3]));
                material.SetTexture("_BaseMap", LoadTexture(source.albedo_png, false, slot != "atlas"));
                material.SetTexture("_NormalMap", LoadTexture(source.normal_png, true, slot != "atlas"));
                material.SetTexture("_Masks", LoadTexture(source.masks_png, true, slot != "atlas"));
                material.SetFloat("_UseMasks", string.IsNullOrEmpty(source.masks_png) ? 0f : 1f);
                if (slot != "atlas" && source.physical_size != null && source.physical_size.Length == 2)
                    material.SetTextureScale("_BaseMap", new Vector2(2f / Mathf.Max(0.001f, source.physical_size[0]),
                        2f / Mathf.Max(0.001f, source.physical_size[1])));
                ApplyGrub(material, source.grub_uniform);
            }
            if (slot == "glass" || slot == "window_proxy")
            {
                material.SetFloat("_SrcBlend", (float)BlendMode.SrcAlpha);
                material.SetFloat("_DstBlend", (float)BlendMode.OneMinusSrcAlpha);
                material.SetFloat("_ZWrite", 0);
                material.renderQueue = (int)RenderQueue.Transparent;
                material.SetFloat("_Roughness", 0.15f);
                material.SetShaderPassEnabled("ShadowCaster", false);
            }
            if (slot == "hardware") { material.SetFloat("_Metallic", 0.8f); material.SetFloat("_Roughness", 0.35f); }
            if (slot.StartsWith("thatch_shell", StringComparison.Ordinal)) material.SetFloat("_Cutoff", 0.5f);
            return material;
        }

        private static void ApplyGrub(Material material, float[] uniform)
        {
            if (uniform == null || uniform.Length != 20) return;
            for (var index = 0; index < 5; index++)
            {
                var offset = index * 4;
                material.SetVector("_Grub" + index, new Vector4(uniform[offset],
                    uniform[offset + 1], uniform[offset + 2], uniform[offset + 3]));
            }
        }

        private Texture2D LoadTexture(string png, bool linear, bool repeat)
        {
            if (string.IsNullOrEmpty(png)) return null;
            var texture = Own(new Texture2D(2, 2, TextureFormat.RGBA32, true, linear));
            if (!ImageConversion.LoadImage(texture, Convert.FromBase64String(png), false))
                throw new InvalidOperationException("A native town texture PNG could not be decoded.");
            texture.wrapMode = repeat ? TextureWrapMode.Repeat : TextureWrapMode.Clamp;
            texture.anisoLevel = 4;
            return texture;
        }

        private T Own<T>(T value) where T : UnityEngine.Object { owned.Add(value); return value; }
        internal static Vector3 Vector(float[] data, int offset = 0) => new Vector3(data[offset], data[offset + 2], data[offset + 1]);
        internal static Vector3 Position(float[] native, float worldSize) => Vector(native) - new Vector3(worldSize * 0.5f, 0f, worldSize * 0.5f);
        private void OnDestroy() { foreach (var value in owned) DestroyUnityObject(value); owned.Clear(); }
    }
}
