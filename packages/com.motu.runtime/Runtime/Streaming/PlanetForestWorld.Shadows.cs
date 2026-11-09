using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

namespace Motu.Streaming
{
    internal static partial class PlanetForestWorld
    {
        // Match URP's per-camera range without adding components to cameras.
        internal static float ShadowDistance(Camera camera)
        {
            if (!(GraphicsSettings.currentRenderPipeline is UniversalRenderPipelineAsset pipeline)
                || !(pipeline.supportsMainLightShadows || pipeline.supportsAdditionalLightShadows)) return 0;
            if (camera.cameraType != CameraType.SceneView
                && camera.TryGetComponent<UniversalAdditionalCameraData>(out var data) && !data.renderShadows) return 0;
            var distance = Mathf.Min(pipeline.shadowDistance, camera.farClipPlane);
            return distance >= camera.nearClipPlane ? distance : 0;
        }

        private static bool InShadowRange(View view, Bounds bounds) =>
            view.shadowDistanceSquared > 0 && bounds.SqrDistance(view.shadowEye) <= view.shadowDistanceSquared;

        private static ShadowCastingMode FarShadowMode(View view, Bounds bounds, bool visible) =>
            !InShadowRange(view, bounds) ? ShadowCastingMode.Off
                : visible ? ShadowCastingMode.On : ShadowCastingMode.ShadowsOnly;

        private static void SubmitFarTrees(View view, Camera camera)
        {
            foreach (var forest in Forests)
            {
                if ((camera.cullingMask & (1 << forest.gameObject.layer)) == 0) continue;
                var visible = GeometryUtility.TestPlanesAABB(view.planes, forest.WorldBounds);
                if (!visible && !InShadowRange(view, forest.WorldBounds)) continue;
                foreach (var batch in forest.batches)
                {
                    var batchVisible = visible && GeometryUtility.TestPlanesAABB(view.planes, batch.worldBounds);
                    var mayCast = InShadowRange(view, batch.worldBounds);
                    if (!batchVisible && !mayCast) continue;
                    SubmitFarBatch(view, camera, forest, batch, batchVisible, mayCast);
                }
            }
        }

        private static void SubmitFarBatch(View view, Camera camera, InstancedForestRenderer forest,
            InstancedForestRenderer.Batch batch, bool visible, bool mayCast)
        {
            foreach (var index in batch.indices)
            {
                if (view.selected.Contains(new Tree(forest, index, 0))) continue;
                var bounds = batch.worldBounds;
                if (mayCast)
                {
                    bounds = InstancedForestRenderer.TransformBounds(forest.World[index], forest.Models.bounds[batch.prototype]);
                    bounds.Expand(1f);
                }
                var shadows = mayCast ? FarShadowMode(view, bounds, visible) : ShadowCastingMode.Off;
                if (!visible && shadows == ShadowCastingMode.Off) continue;
                Add(forest, index, 4, bounds, camera, forest.Models.IsPalm(batch.prototype), shadows);
                if (visible) CountFarTree(forest, batch.prototype);
            }
        }

        private static void CountFarTree(InstancedForestRenderer forest, int prototype)
        {
            ++forest.SubmittedFarTrees;
            if (forest.Models.IsPine(prototype))
            {
                forest.LastPineFoliageKind = 3;
                forest.LastPineFoliageLod = 2;
            }
            if (forest.Models.IsPalm(prototype))
            {
                forest.LastPalmFoliageLod = 2;
                ++forest.SubmittedPalmFarTrees;
            }
        }
    }
}
