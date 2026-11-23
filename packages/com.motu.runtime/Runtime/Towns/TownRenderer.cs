using System;
using System.Collections.Generic;
using Motu.Rendering;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Towns
{
    /// <summary>Imports native results only. Placement and terrain fitting live in Rust.</summary>
    public sealed partial class TownRenderer : MonoBehaviour
    {
        private readonly List<UnityEngine.Object> owned = new List<UnityEngine.Object>();
        private readonly List<TownBuildingLod> buildingLods = new List<TownBuildingLod>();
        private Vector4? grubDirtColour;
        private MeshRenderer roads;
        private TownDecorationCatalogue decorationCatalogue;
        private TownBridgeCandidate[] bridgeCandidates = Array.Empty<TownBridgeCandidate>();
        private TownCartTrackBridge[] cartTrackBridges = Array.Empty<TownCartTrackBridge>();
        private const float NativeWorldSizeMetres = 2000f;
        internal GameObject[] SharedPrototypes { get; private set; } = Array.Empty<GameObject>();
        internal IReadOnlyList<TownBuildingLod> BuildingLods => buildingLods;
        /// <summary>Capital and villages with at least one placed building.</summary>
        public int SettlementCount { get; private set; }
        public int BuildingCount { get; private set; }
        public int RoadCount { get; private set; }
        public int RoadFrontageCount { get; private set; }
        public int BridgeCandidateCount => bridgeCandidates.Length;
        public int CartTrackRunCount { get; private set; }
        public int CartTrackLinkCount { get; private set; }
        public int CartTrackBridgeCandidateCount => cartTrackBridges.Length;
        public Vector3 MouthLocalPosition { get; private set; }
        public Vector3 InspectionLocalPosition { get; private set; }

        internal static (TownRenderer owner, GameObject building) BuildDecorationPreview(Transform parent, TownPrototype source)
        {
            var owner = new GameObject("Recipe preview") { hideFlags = HideFlags.HideAndDontSave };
            owner.transform.SetParent(parent, false);
            var renderer = owner.AddComponent<TownRenderer>();
            try
            {
                Material churchStone = null;
                var building = renderer.BuildPrototype(owner.transform, source, out _, ref churchStone);
                building.GetComponent<LODGroup>().ForceLOD(0);
                foreach (var child in building.GetComponentsInChildren<Transform>(true))
                    child.gameObject.hideFlags = HideFlags.DontSave;
                foreach (var resource in renderer.owned) resource.hideFlags = HideFlags.HideAndDontSave;
                return (renderer, building);
            }
            catch { renderer.DestroyDecorationPreview(); throw; }
        }

        internal void DestroyDecorationPreview()
        {
            // This preview renderer is built in Edit Mode and has no runtime lifecycle callbacks.
            ReleaseOwnedResources();
            DestroyUnityObject(gameObject);
        }

        internal static TownRenderer Install(Transform parent, TownPreparedData data, float worldSize,
            bool includeUnplacedPrototypes = false, Material roadMaterial = null,
            TerrainMaterialTextureArrays terrainTextures = null, bool roadsOnTerrain = false,
            bool useBuildingLodBudget = false, Color? terrainDirtColour = null,
            TownDecorationCatalogue decorationCatalogue = null, Font roadSignFont = null)
        {
            if (data == null || (!includeUnplacedPrototypes && (data.instances?.Length ?? 0) == 0
                && (data.settlements?.Length ?? 0) == 0
                && (data.road_mesh?.indices?.Length ?? 0) == 0 && (data.bridge_candidates?.Length ?? 0) == 0
                && (data.cartTracks?.run_count ?? 0) == 0
                && (data.cartTracks?.departures?.Length ?? 0) == 0
                && (data.bridgePlan?.bridges?.Length ?? 0) == 0
                && (data.bridgePlan?.diagnostics?.Length ?? 0) == 0 && data.pier == null
                && (data.roadsideFields?.fields?.Length ?? 0) == 0)) return null;
            var root = new GameObject((data.settlements?.Length ?? 0) > 0 ? "River-mouth settlements" : "River-mouth town");
            root.transform.SetParent(parent, false);
            root.transform.localScale = Vector3.one * (worldSize / NativeWorldSizeMetres);
            var renderer = root.AddComponent<TownRenderer>();
            renderer.decorationCatalogue = decorationCatalogue;
            // Grub uses a vector uniform. Match the terrain colour property's
            // conversion on upload in a linear colour-space project.
            if (terrainDirtColour is Color dirt)
                renderer.grubDirtColour = QualitySettings.activeColorSpace == ColorSpace.Linear ? dirt.linear : dirt;
            try { renderer.Build(data, roadMaterial, terrainTextures, roadsOnTerrain, useBuildingLodBudget, roadSignFont); return renderer; }
            catch
            {
                renderer.ReleaseOwnedResources();
                DestroyUnityObject(root);
                throw;
            }
        }

        private void Build(TownPreparedData data, Material roadMaterial, TerrainMaterialTextureArrays terrainTextures,
            bool roadsOnTerrain, bool useBuildingLodBudget, Font roadSignFont)
        {
            var templatesRoot = new GameObject("Shared building prototypes");
            templatesRoot.transform.SetParent(transform, false);
            templatesRoot.SetActive(false);
            var settlements = data.settlements ?? Array.Empty<TownSettlement>();
            var capitalIndex = Array.FindIndex(settlements, settlement => settlement.kind == "capital");
            MouthLocalPosition = GroundPosition(data.mouth, Vector3.zero);
            InspectionLocalPosition = capitalIndex >= 0
                ? GroundPosition(settlements[capitalIndex].centre, MouthLocalPosition)
                : MouthLocalPosition;
            var churchStone = BuildBuildings(data, templatesRoot.transform, BuildSettlementRoots(settlements),
                capitalIndex, useBuildingLodBudget);
            if ((data.roadsideFields?.walls?.Length ?? 0) > 0)
                churchStone ??= CreateMaterial(TownNative.ChurchStone, "Shared settlement stone");
            if (settlements.Length == 0 && BuildingCount > 0) SettlementCount = 1;
            BuildSupports(data);
            BuildBridges(data.bridgePlan, churchStone);
            BuildCapitalPier(data.pier, data.pierDiagnostic, churchStone);
            BuildRoadsideFields(data.roadsideFields, churchStone);
            BuildCartTrackSigns(data.cartTracks?.departures, roadSignFont);
            RoadCount = data.road_count;
            bridgeCandidates = data.bridge_candidates ?? Array.Empty<TownBridgeCandidate>();
            CartTrackRunCount = data.cartTracks?.run_count ?? 0;
            CartTrackLinkCount = data.cartTracks?.links?.Length ?? 0;
            cartTrackBridges = data.cartTracks?.bridges ?? Array.Empty<TownCartTrackBridge>();
            if (!roadsOnTerrain && data.road_mesh?.indices?.Length > 0)
            {
                BuildRoads(data.road_mesh, roadMaterial, terrainTextures);
                if (settlements.Length == 0 && BuildingCount == 0)
                    InspectionLocalPosition = roads.localBounds.center + roads.transform.localPosition;
            }
        }

        private Transform[] BuildSettlementRoots(TownSettlement[] settlements)
        {
            var roots = new Transform[settlements.Length];
            for (var index = 0; index < roots.Length; index++)
            {
                var settlement = settlements[index];
                var label = settlement.kind == "capital" ? "Capital" : "Village";
                var root = new GameObject($"{label} (river {settlement.river_index})");
                root.transform.SetParent(transform, false);
                root.transform.localPosition = GroundPosition(settlement.centre, GroundPosition(settlement.mouth, Vector3.zero));
                roots[index] = root.transform;
                if (settlement.building_count > 0) SettlementCount++;
            }
            return roots;
        }

        private Material BuildBuildings(TownPreparedData data, Transform templatesRoot, Transform[] settlementRoots,
            int capitalIndex, bool useBuildingLodBudget)
        {
            var prototypes = new GameObject[data.prototypes.Length];
            var prototypeBounds = new Bounds[prototypes.Length];
            Material churchStone = null;
            SharedPrototypes = prototypes;
            for (var index = 0; index < prototypes.Length; index++)
                prototypes[index] = BuildPrototype(templatesRoot, data.prototypes[index], out prototypeBounds[index],
                    ref churchStone);
            var grouped = settlementRoots.Length > 0;
            var sawInspectionBuilding = false;
            var sawInspectionFrontage = false;
            foreach (var instance in data.instances ?? Array.Empty<TownInstance>())
            {
                if ((uint)instance.prototype_index >= (uint)prototypes.Length)
                    throw new InvalidOperationException("The native town refers to an invalid building prototype.");
                if (grouped && (uint)instance.settlement_index >= (uint)settlementRoots.Length)
                    throw new InvalidOperationException("The native building refers to an invalid coastal settlement.");
                var parent = grouped ? settlementRoots[instance.settlement_index] : transform;
                var building = Instantiate(prototypes[instance.prototype_index], parent, false);
                building.name = string.IsNullOrEmpty(instance.entry_id)
                    ? data.prototypes[instance.prototype_index].entry_id : instance.entry_id;
                var position = Position(instance.position, NativeWorldSizeMetres);
                building.transform.localPosition = grouped ? position - parent.localPosition : position;
                building.transform.localRotation = Quaternion.Euler(0f, -instance.yaw_radians * Mathf.Rad2Deg, 0f);
                var decorations = building.GetComponent<TownDecorationAnchors>();
                decorations?.ConfigurePlaced(decorationCatalogue, data.seed, building.name,
                    Vector(instance.position), useBuildingLodBudget);
                if (useBuildingLodBudget)
                    buildingLods.Add(new TownBuildingLod(building.GetComponent<LODGroup>(),
                        prototypeBounds[instance.prototype_index], decorations));
                building.SetActive(true);
                BuildingCount++;

                var inspect = !grouped || instance.settlement_index == capitalIndex;
                if (inspect && !sawInspectionBuilding)
                {
                    InspectionLocalPosition = position;
                    sawInspectionBuilding = true;
                }
                if (instance.frontage?.street_edge?.Length == 3)
                {
                    RoadFrontageCount++;
                    if (inspect && !sawInspectionFrontage)
                    {
                        InspectionLocalPosition = Position(instance.frontage.street_edge, NativeWorldSizeMetres);
                        sawInspectionFrontage = true;
                    }
                }
            }
            return churchStone;
        }

        private void BuildSupports(TownPreparedData data)
        {
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
                support.transform.localPosition = new Vector3(-NativeWorldSizeMetres * 0.5f, 0f, -NativeWorldSizeMetres * 0.5f);
                support.AddComponent<MeshCollider>().sharedMesh = mesh;
            }
        }

        private static Vector3 GroundPosition(float[] position, Vector3 fallback) => position?.Length == 2
            ? new Vector3(position[0] - NativeWorldSizeMetres * 0.5f, 0f, position[1] - NativeWorldSizeMetres * 0.5f)
            : fallback;

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
        }

        internal void SetRoadVisibility(bool visible)
        {
            if (roads != null) roads.enabled = visible;
        }

        internal void ReleaseDetailedBuildingLods()
        {
            foreach (var building in buildingLods)
                building.SetLevel(TownBuildingLod.PaintedShell);
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

        public bool TryGetCartTrackBridgeCandidate(int index, out Vector3 from, out Vector3 to, out string kind)
        {
            from = to = default;
            kind = null;
            if ((uint)index >= (uint)cartTrackBridges.Length) return false;
            var candidate = cartTrackBridges[index];
            if (candidate?.start?.Length != 3 || candidate.end?.Length != 3) return false;
            from = transform.TransformPoint(Position(candidate.start, NativeWorldSizeMetres));
            to = transform.TransformPoint(Position(candidate.end, NativeWorldSizeMetres));
            kind = candidate.kind;
            return true;
        }

        private GameObject BuildPrototype(Transform parent, TownPrototype source, out Bounds shellBounds,
            ref Material churchStone)
        {
            var root = new GameObject(source.entry_id + " [" + source.hash + "]");
            root.transform.SetParent(parent, false);
            var materials = new Dictionary<string, Material>(StringComparer.Ordinal);
            foreach (var material in source.materials ?? Array.Empty<TownMaterial>())
            {
                var imported = CreateMaterial(material, material.slot);
                imported.name = source.entry_id + " " + material.slot;
                materials[material.slot] = imported;
                if (source.is_church && material.slot == "wall" && material.grub_uniform?.Length == 20)
                    churchStone ??= imported;
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
            shellBounds = default;
            for (var level = 0; level < source.lods.Length; level++)
            {
                var renderers = new List<Renderer>();
                foreach (var part in source.lods[level].parts)
                {
                    var mesh = CreateMesh(part.mesh, source.entry_id + " LOD" + level + " " + part.material);
                    if (level == lods.Length - 1)
                    {
                        if (renderers.Count == 0) shellBounds = mesh.bounds;
                        else shellBounds.Encapsulate(mesh.bounds);
                    }
                    renderers.Add(AddMesh(root.transform, mesh, Resolve(part.material), mesh.name).GetComponent<Renderer>());
                }
                lods[level] = new LOD(level == 0 ? 0.3f : level == 1 ? 0.08f : 0f, renderers.ToArray());
            }
            AddBoxes(root, source.colliders);
            AddBoxes(root, source.support_colliders);
            foreach (var door in source.doors ?? Array.Empty<TownDoorData>())
                AddDoor(root.transform, door, Resolve("door"), Resolve("hardware"));
            // Native coarse LODs already contain their closed-door appearance.
            // The shared prototype is inactive while its renderers are assigned.
            var nearRenderers = new List<Renderer>(lods[0].renderers);
            foreach (var door in root.GetComponentsInChildren<TownDoor>(true))
                nearRenderers.AddRange(door.GetComponentsInChildren<Renderer>(true));
            lods[0].renderers = nearRenderers.ToArray();
            var group = root.AddComponent<LODGroup>();
            group.SetLODs(lods);
            group.RecalculateBounds();
            if (source.decoration_anchors?.Length > 0)
                root.AddComponent<TownDecorationAnchors>().Import(source.decoration_anchors);
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

        internal Mesh CreateMesh(TownMesh source, string name, Vector3 origin = default)
            => CreateMesh(name, origin, 1f, source);

        private Mesh CreateMesh(string name, Vector3 origin, float uvScale, params TownMesh[] sources)
        {
            var count = 0;
            var indexCount = 0;
            foreach (var source in sources)
            {
                if (source?.positions == null || source.positions.Length % 3 != 0)
                    throw new InvalidOperationException("The native town returned invalid mesh positions.");
                count += source.positions.Length / 3;
                indexCount += source.indices.Length;
            }
            var positions = new Vector3[count];
            var normals = new Vector3[count];
            var uv = new Vector2[count];
            var triangles = new int[indexCount];
            var vertexBase = 0;
            var indexBase = 0;
            foreach (var source in sources)
            {
                var vertexCount = source.positions.Length / 3;
                for (var i = 0; i < vertexCount; i++)
                {
                    positions[vertexBase + i] = Vector(source.positions, i * 3) - origin;
                    normals[vertexBase + i] = Vector(source.normals, i * 3);
                    uv[vertexBase + i] = new Vector2(source.uvs[i * 2], source.uvs[i * 2 + 1]) * uvScale;
                }
                // Swapping native Y/Z changes handedness.
                for (var i = 0; i < source.indices.Length; i += 3)
                {
                    triangles[indexBase + i] = vertexBase + source.indices[i];
                    triangles[indexBase + i + 1] = vertexBase + source.indices[i + 2];
                    triangles[indexBase + i + 2] = vertexBase + source.indices[i + 1];
                }
                vertexBase += vertexCount;
                indexBase += source.indices.Length;
            }
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
            if (slot == "atlas")
            {
                // Native atlas charts address PNG rows from the top. Unity samples from the bottom.
                material.SetTextureScale("_BaseMap", new Vector2(1f, -1f));
                material.SetTextureOffset("_BaseMap", new Vector2(0f, 1f));
            }
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

        private void ApplyGrub(Material material, float[] uniform)
        {
            if (uniform == null || uniform.Length != 20) return;
            for (var index = 0; index < 5; index++)
            {
                var offset = index * 4;
                var authored = new Vector4(uniform[offset], uniform[offset + 1], uniform[offset + 2], uniform[offset + 3]);
                if (index == 1) SetGrubDirtColour(material, authored);
                else material.SetVector("_Grub" + index, authored);
            }
        }

        private void SetGrubDirtColour(Material material, Vector4 authored) =>
            material.SetVector("_Grub1", grubDirtColour ?? authored);

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
        private void OnDestroy() => ReleaseOwnedResources();

        internal void ReleaseOwnedResources()
        {
            foreach (var value in owned) DestroyUnityObject(value);
            owned.Clear();
            shrubNoise?.Dispose();
            shrubNoise = null;
        }
    }
}
