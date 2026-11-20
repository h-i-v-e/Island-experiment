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
        private static readonly int WheatTileFadeId = Shader.PropertyToID("_WheatTileFade");
        private const float WheatHandoffSeconds = 0.18f;
        private readonly Dictionary<Vector2Int, WheatHandoff> wheatHandoffs = new Dictionary<Vector2Int, WheatHandoff>();
        private readonly List<Vector2Int> completedWheatHandoffs = new List<Vector2Int>();
        private MaterialPropertyBlock wheatFadeProperties;
        private WheatField[] wheatFields;
        private Material wheatCanopyMaterial;
        private Texture2D wheatMaskTexture;
        private WheatClumpRenderer wheatClumps;

        private readonly struct WheatHandoff
        {
            internal readonly MeshRenderer Canopy;
            internal readonly Mesh Mesh;
            internal readonly bool ToPlants;
            internal readonly float StartTime;

            internal WheatHandoff(MeshRenderer canopy, Mesh mesh, bool toPlants)
            {
                Canopy = canopy;
                Mesh = mesh;
                ToPlants = toPlants;
                StartTime = Time.unscaledTime;
            }
        }

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

        private void InstallWheatTile(Tile tile, WheatTileData data)
        {
            if (data == null) return;
            tile.wheatPlants = data.Plants;
            if (data.Canopy == null) return;
            var mesh = IslandMeshInterop.CreateGeneratedMesh(data.Canopy);
            tile.wheatCanopy = mesh;
            tile.wheatRenderer = CreateCanopyRenderer(tile.gameObject.transform, mesh);
        }

        private MeshRenderer CreateCanopyRenderer(Transform parent, Mesh mesh)
        {
            var host = new GameObject("Wheat field canopy") { layer = parent.gameObject.layer };
            host.transform.SetParent(parent, false);
            host.AddComponent<MeshFilter>().sharedMesh = mesh;
            var renderer = host.AddComponent<MeshRenderer>();
            renderer.sharedMaterial = wheatCanopyMaterial;
            renderer.shadowCastingMode = ShadowCastingMode.Off;
            return renderer;
        }

        private Tile FindWheatCanopy(Vector2Int key)
        {
            var parent = new Vector2Int(key.x / Divisions, key.y / Divisions);
            if (!lod1Groups.TryGetValue(parent, out var group)) return null;
            return group.tiles[(key.y % Divisions) * Divisions + key.x % Divisions];
        }

        private void ShowWheatPlants(Vector2Int key, WheatPlant[] plants, bool fromCanopy)
        {
            if (wheatClumps == null) return;
            FinishWheatHandoff(key);
            wheatClumps.SetTile(key, plants);
            if (!fromCanopy || !Application.isPlaying || (plants?.Length ?? 0) == 0) return;
            var tile = FindWheatCanopy(key);
            if (tile?.wheatCanopy == null) return;
            // Borrow the outgoing mesh for the dissolve. The terrain tile can
            // retire on schedule and the canopy uses the same island transform.
            var renderer = CreateCanopyRenderer(transform, tile.wheatCanopy);
            wheatHandoffs.Add(key, new WheatHandoff(renderer, tile.wheatCanopy, true));
            wheatClumps.SetTileFade(key, 0);
        }

        private void HideWheatPlants(Vector2Int key, WheatPlant[] plants)
        {
            if (wheatClumps == null) return;
            FinishWheatHandoff(key);
            var tile = FindWheatCanopy(key);
            if (!Application.isPlaying || (plants?.Length ?? 0) == 0 || tile?.wheatRenderer == null)
            {
                wheatClumps.SetTile(key, null);
                return;
            }
            wheatHandoffs.Add(key, new WheatHandoff(tile.wheatRenderer, tile.wheatCanopy, false));
            SetWheatTileFade(tile.wheatRenderer, 0);
        }

        private void UpdateWheatHandoffs()
        {
            if (wheatHandoffs.Count == 0) return;
            completedWheatHandoffs.Clear();
            foreach (var entry in wheatHandoffs)
            {
                var handoff = entry.Value;
                var progress = (Time.unscaledTime - handoff.StartTime) / WheatHandoffSeconds;
                if (progress >= 1 || handoff.Mesh == null || handoff.Canopy == null)
                {
                    completedWheatHandoffs.Add(entry.Key);
                    continue;
                }
                var plants = handoff.ToPlants ? progress : 1f - progress;
                wheatClumps.SetTileFade(entry.Key, plants);
                SetWheatTileFade(handoff.Canopy, 1f - plants);
            }
            foreach (var key in completedWheatHandoffs) FinishWheatHandoff(key);
        }

        private void SetWheatTileFade(MeshRenderer canopy, float fade)
        {
            wheatFadeProperties ??= new MaterialPropertyBlock();
            wheatFadeProperties.SetFloat(WheatTileFadeId, fade);
            canopy.SetPropertyBlock(wheatFadeProperties);
        }

        private void FinishWheatHandoff(Vector2Int key)
        {
            if (!wheatHandoffs.Remove(key, out var handoff)) return;
            if (handoff.ToPlants)
            {
                if (handoff.Canopy != null)
                {
                    handoff.Canopy.enabled = false;
                    DestroyUnityObject(handoff.Canopy.gameObject);
                }
                wheatClumps.SetTileFade(key, 1f);
            }
            else
            {
                if (handoff.Canopy != null) handoff.Canopy.SetPropertyBlock(null);
                wheatClumps.SetTile(key, null);
            }
        }

        internal void SetWheatHorizonDistance(float distance, float switchDistance)
        {
            if (wheatCanopyMaterial != null)
                wheatCanopyMaterial.SetFloat(WheatFadeId, 1f - Mathf.InverseLerp(
                    Mathf.Max(0f, switchDistance - 120f), switchDistance, distance));
        }

        private void ClearWheatFocus()
        {
            if (wheatClumps == null) return;
            completedWheatHandoffs.Clear();
            completedWheatHandoffs.AddRange(wheatHandoffs.Keys);
            foreach (var key in completedWheatHandoffs) FinishWheatHandoff(key);
            wheatClumps.Clear();
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
