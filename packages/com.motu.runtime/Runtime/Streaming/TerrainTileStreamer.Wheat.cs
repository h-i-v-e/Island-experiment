using System.Collections.Generic;
using Motu.Interop;
using Motu.Islands;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Streaming
{
    public sealed partial class TerrainTileStreamer
    {
        private static readonly int WheatFadeId = Shader.PropertyToID("_WheatFade");
        private static readonly int WheatDetailBlendId = Shader.PropertyToID("_WheatDetailBlend");
        private static readonly int WheatDetailPresenceId = Shader.PropertyToID("_WheatDetailPresence");
        private const float WheatRevealSeconds = 0.18f;
        private readonly Dictionary<Vector2Int, float> wheatReveals = new Dictionary<Vector2Int, float>();
        private readonly List<Vector2Int> finishedWheatReveals = new List<Vector2Int>();
        private MaterialPropertyBlock wheatFadeProperties;
        private WheatField[] wheatFields;
        private Material wheatCanopyMaterial;
        private Texture2D wheatMaskTexture;
        private WheatClumpRenderer wheatClumps;

        private void InstallWheat(IslandPreparedWheat wheat)
        {
            if (wheat == null) return;
            wheatFields = wheat.Fields;
            var canopy = Resources.Load<Shader>("WheatCanopy")
                ?? throw new System.InvalidOperationException("The wheat canopy shader is unavailable.");
            var clumps = Resources.Load<Material>("Motu/Variants/WheatClumps")
                ?? throw new System.InvalidOperationException("The wheat clump material is unavailable.");
            wheatCanopyMaterial = new Material(canopy) { name = "Wheat fields" };
            wheatMaskTexture = wheat.Mask?.Bind(terrainMaterial, terrainLod1Material, terrainLod2Material, grassMaterial);
            wheatClumps = new WheatClumpRenderer(transform, clumps);
        }

        private void InstallWheatTile(Tile tile, WheatTileData data, int lod)
        {
            if (data == null) return;
            tile.wheatPlants = data.Plants;
            if (data.Canopy == null) return;
            var mesh = IslandMeshInterop.CreateGeneratedMesh(data.Canopy);
            tile.wheatCanopy = mesh;
            // LOD1 wheat stays with its group while the terrain tile switches to LOD0.
            var tileTransform = tile.gameObject.transform;
            var parent = lod == 1 ? tileTransform.parent : tileTransform;
            var host = new GameObject("Wheat field canopy") { layer = tile.gameObject.layer };
            host.transform.SetParent(parent, false);
            host.AddComponent<MeshFilter>().sharedMesh = mesh;
            var renderer = host.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = wheatCanopyMaterial;
            renderer.shadowCastingMode = ShadowCastingMode.Off;
            tile.wheatRenderer = renderer;
        }

        private void SetWheatFocus(Vector3 worldPosition)
        {
            if (wheatCanopyMaterial == null) return;
            // Even from the edge of the centre tile, the surrounding LOD0 ring
            // extends one tile farther. Finish fading inside that distance.
            float tileWidth = worldSize / Lod1Resolution;
            var blend = new Vector4(worldPosition.x, worldPosition.z, tileWidth * .55f, tileWidth * .95f);
            wheatCanopyMaterial.SetVector(WheatDetailBlendId, blend);
            wheatClumps.DetailBlend = blend;
        }

        private MeshRenderer FindWheatCanopy(Vector2Int key)
        {
            var parent = new Vector2Int(key.x / Divisions, key.y / Divisions);
            if (!lod1Groups.TryGetValue(parent, out var group)) return null;
            return group.tiles[(key.y % Divisions) * Divisions + key.x % Divisions]?.wheatRenderer;
        }

        private void ShowWheatPlants(Vector2Int key, WheatPlant[] plants, bool newTile)
        {
            if (wheatClumps == null) return;
            wheatClumps.SetTile(key, plants);
            if ((plants?.Length ?? 0) > 0 && newTile && Application.isPlaying && FindWheatCanopy(key) != null)
                wheatReveals[key] = Time.unscaledTime;
        }

        private void HideWheatPlants(Vector2Int key)
        {
            wheatReveals.Remove(key);
            wheatClumps?.SetTile(key, null);
        }

        private void SyncWheatCanopy(Vector2Int key, MeshRenderer canopy, bool hasDetail)
        {
            if (wheatClumps == null) return;
            bool blend = canopy != null && hasDetail && lod0Groups.TryGetValue(key, out var group)
                && (group.tiles[0]?.wheatPlants?.Length ?? 0) > 0;
            wheatClumps.SetTileBlend(key, blend);
            if (!blend)
            {
                if (hasDetail)
                {
                    wheatReveals.Remove(key);
                    wheatClumps.SetTileFade(key, 1f);
                }
                canopy?.SetPropertyBlock(null);
                return;
            }
            float presence = wheatReveals.TryGetValue(key, out var start) ? WheatRevealProgress(start) : 1f;
            SetWheatPresence(key, canopy, presence);
        }

        private void UpdateWheatReveals()
        {
            if (wheatReveals.Count == 0) return;
            finishedWheatReveals.Clear();
            foreach (var entry in wheatReveals)
            {
                var canopy = FindWheatCanopy(entry.Key);
                float progress = canopy == null ? 1f : WheatRevealProgress(entry.Value);
                if (canopy == null) wheatClumps.SetTileBlend(entry.Key, false);
                SetWheatPresence(entry.Key, canopy, progress);
                if (progress >= 1f) finishedWheatReveals.Add(entry.Key);
            }
            foreach (var key in finishedWheatReveals) wheatReveals.Remove(key);
        }

        private static float WheatRevealProgress(float start) => Mathf.Clamp01((Time.unscaledTime - start) / WheatRevealSeconds);

        private void SetWheatPresence(Vector2Int key, MeshRenderer canopy, float presence)
        {
            wheatClumps.SetTileFade(key, presence);
            if (canopy == null) return;
            wheatFadeProperties ??= new MaterialPropertyBlock();
            wheatFadeProperties.SetFloat(WheatDetailPresenceId, presence);
            canopy.SetPropertyBlock(wheatFadeProperties);
        }

        internal void SetWheatHorizonDistance(float distance, float switchDistance)
        {
            if (wheatCanopyMaterial != null)
                wheatCanopyMaterial.SetFloat(WheatFadeId, 1f - Mathf.InverseLerp(
                    Mathf.Max(0f, switchDistance - 120f), switchDistance, distance));
        }

        private void ClearWheatFocus()
        {
            wheatReveals.Clear();
            wheatClumps?.Clear();
        }

        private void DisposeWheat()
        {
            wheatClumps?.Dispose();
            wheatClumps = null;
            DestroyUnityObject(wheatCanopyMaterial);
            DestroyUnityObject(wheatMaskTexture);
            wheatCanopyMaterial = null;
            wheatMaskTexture = null;
            wheatFields = null;
        }
    }
}
