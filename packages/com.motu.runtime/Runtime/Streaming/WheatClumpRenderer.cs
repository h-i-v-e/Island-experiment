using System;
using System.Collections.Generic;
using Motu.Islands;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // Terrain LOD0 ownership determines which tiles have draw batches. Every
    // batch uses the same mesh, including batches from different islands.
    internal sealed class WheatClumpRenderer : IDisposable
    {
        private const int BatchLimit = 128;
        private const float MaximumWindFlexibility = 1.25f;
        private static readonly int WindMaterialId = Shader.PropertyToID("_MotuWindMaterial");
        private static readonly int WeatherWindId = Shader.PropertyToID("_MotuWeatherWind");
        private static readonly int DataId = Shader.PropertyToID("_WheatInstanceData");
        private static Mesh sharedMesh;
        private static int meshUsers;

        private readonly Transform owner;
        private readonly Material material;
        private readonly Dictionary<Vector2Int, Batch[]> tiles = new Dictionary<Vector2Int, Batch[]>();
        private Matrix4x4 previousMatrix;
        private bool disposed;

        internal WheatClumpRenderer(Transform owner, Material material)
        {
            if (owner == null) throw new ArgumentNullException(nameof(owner));
            if (material == null) throw new ArgumentNullException(nameof(material));
            this.owner = owner;
            this.material = material;
            material.enableInstancing = true;
            previousMatrix = owner.localToWorldMatrix;
            if (sharedMesh == null) sharedMesh = CreateMesh();
            ++meshUsers;
            RenderPipelineManager.beginCameraRendering += RenderCamera;
        }

        internal void SetTile(Vector2Int key, WheatPlant[] plants)
        {
            if (disposed) return;
            if (plants == null || plants.Length == 0)
            {
                tiles.Remove(key);
                return;
            }

            var batches = new Batch[(plants.Length + BatchLimit - 1) / BatchLimit];
            for (var i = 0; i < batches.Length; ++i)
            {
                var offset = i * BatchLimit;
                var count = Math.Min(BatchLimit, plants.Length - offset);
                batches[i] = new Batch(plants, offset, count, sharedMesh.bounds, previousMatrix);
            }
            tiles[key] = batches;
        }

        internal void SetTileFade(Vector2Int key, float fade)
        {
            if (!tiles.TryGetValue(key, out var batches)) return;
            fade = Mathf.Clamp01(fade);
            foreach (var batch in batches) batch.SetFade(fade);
        }

        internal void Clear() => tiles.Clear();

        private void RenderCamera(ScriptableRenderContext context, Camera camera)
        {
            if (tiles.Count == 0 || owner == null || material == null || camera == null
                || !owner.gameObject.activeInHierarchy) return;
            var layer = owner.gameObject.layer;
            if ((camera.cullingMask & (1 << layer)) == 0) return;

            var matrix = owner.localToWorldMatrix;
            var moved = matrix != previousMatrix;
            previousMatrix = matrix;
            // Match the maximum wind displacement in the vertex shader, so
            // a bent tip remains inside Unity's view and shadow culling bounds.
            var windPadding = MaximumWindFlexibility * Mathf.Max(0, Shader.GetGlobalVector(WindMaterialId).x)
                * Mathf.Max(0, Shader.GetGlobalVector(WeatherWindId).w);
            foreach (var batches in tiles.Values)
                foreach (var batch in batches)
                {
                    if (moved) batch.SetTransform(matrix);
                    batch.Submit(camera, material, layer, windPadding);
                }
        }

        public void Dispose()
        {
            if (disposed) return;
            disposed = true;
            RenderPipelineManager.beginCameraRendering -= RenderCamera;
            Clear();
            if (--meshUsers != 0) return;
            DestroyUnityObject(sharedMesh);
            sharedMesh = null;
        }

        private sealed class Batch
        {
            private readonly Matrix4x4[] local;
            private readonly Matrix4x4[] world;
            private readonly Vector4[] instanceData;
            private readonly MaterialPropertyBlock properties = new MaterialPropertyBlock();
            private readonly Bounds localBounds;
            private Bounds worldBounds;

            internal Batch(WheatPlant[] plants, int offset, int count, Bounds meshBounds, Matrix4x4 matrix)
            {
                local = new Matrix4x4[count];
                world = new Matrix4x4[count];
                instanceData = new Vector4[count];
                for (var i = 0; i < count; ++i)
                {
                    var plant = plants[offset + i];
                    // Native yaw turns field X toward +Z in Unity's XZ plane.
                    var rotation = Quaternion.Euler(0, -plant.Yaw * Mathf.Rad2Deg, 0);
                    local[i] = Matrix4x4.TRS(plant.Root, rotation, new Vector3(plant.Width, plant.Height, plant.Width));
                    instanceData[i] = new Vector4(plant.Variation, 1f, 0f, 0f);
                    var bounds = InstancedForestRenderer.TransformBounds(local[i], meshBounds);
                    if (i == 0) localBounds = bounds;
                    else localBounds.Encapsulate(bounds);
                }
                if (SystemInfo.supportsInstancing) properties.SetVectorArray(DataId, instanceData);
                SetTransform(matrix);
            }

            internal void SetFade(float fade)
            {
                if (fade == instanceData[0].y) return;
                for (var i = 0; i < instanceData.Length; ++i) instanceData[i].y = fade;
                if (SystemInfo.supportsInstancing) properties.SetVectorArray(DataId, instanceData);
            }

            internal void SetTransform(Matrix4x4 matrix)
            {
                for (var i = 0; i < world.Length; ++i) world[i] = matrix * local[i];
                worldBounds = InstancedForestRenderer.TransformBounds(matrix, localBounds);
            }

            internal void Submit(Camera camera, Material material, int layer, float windPadding)
            {
                if (instanceData[0].y <= 0f) return;
                if (SystemInfo.supportsInstancing)
                {
                    var bounds = worldBounds;
                    bounds.Expand(new Vector3(windPadding * 2, 0, windPadding * 2));
                    var render = new RenderParams(material)
                    {
                        camera = camera, layer = layer, worldBounds = bounds, matProps = properties,
                        shadowCastingMode = ShadowCastingMode.On, receiveShadows = true, lightProbeUsage = LightProbeUsage.Off
                    };
                    Graphics.RenderMeshInstanced(render, sharedMesh, 0, world);
                    return;
                }

                for (var i = 0; i < world.Length; ++i)
                {
                    properties.SetVector(DataId, instanceData[i]);
                    Graphics.DrawMesh(sharedMesh, world[i], material, layer, camera, 0, properties,
                        ShadowCastingMode.On, true, null, LightProbeUsage.Off);
                }
            }
        }

        private static Mesh CreateMesh()
        {
            var vertices = new List<Vector3>(64);
            var normals = new List<Vector3>(64);
            var uv = new List<Vector2>(64);
            var triangles = new List<int>(144);
            // Four stems per clump. The crossed cards keep the narrow seed
            // heads readable when one of their sides faces the camera edge-on.
            AppendStalk(vertices, normals, uv, triangles, new Vector2(-.28f, -.21f), 1.00f, 16f);
            AppendStalk(vertices, normals, uv, triangles, new Vector2(.23f, -.26f), .83f, 62f);
            AppendStalk(vertices, normals, uv, triangles, new Vector2(.29f, .21f), .96f, 110f);
            AppendStalk(vertices, normals, uv, triangles, new Vector2(-.18f, .26f), .90f, 163f);

            var mesh = new Mesh { name = "Shared wheat clump" };
            mesh.SetVertices(vertices);
            mesh.SetNormals(normals);
            mesh.SetUVs(0, uv);
            mesh.SetTriangles(triangles, 0);
            mesh.UploadMeshData(true);
            return mesh;
        }

        private static void AppendStalk(List<Vector3> vertices, List<Vector3> normals, List<Vector2> uv,
            List<int> triangles, Vector2 root, float height, float angle)
        {
            var origin = new Vector3(root.x, 0, root.y);
            var bend = origin * .32f;
            for (var plane = 0; plane < 2; ++plane)
            {
                var radians = (angle + plane * 90) * Mathf.Deg2Rad;
                var tangent = new Vector3(Mathf.Cos(radians), 0, Mathf.Sin(radians));
                var normal = Vector3.Cross(Vector3.up, tangent);
                for (var level = 0; level < 4; ++level)
                {
                    // Sink the stems slightly into uneven terrain and leave
                    // two intermediate segments for the quadratic wind bend.
                    var v = level == 0 ? -.08f : level / 3f;
                    var center = origin + bend * (v * v) + Vector3.up * (height * v);
                    var row = vertices.Count;
                    vertices.Add(center - tangent * .19f);
                    vertices.Add(center + tangent * .19f);
                    normals.Add(normal);
                    normals.Add(normal);
                    uv.Add(new Vector2(-1, v));
                    uv.Add(new Vector2(1, v));
                    if (level == 0) continue;
                    triangles.Add(row - 2);
                    triangles.Add(row);
                    triangles.Add(row - 1);
                    triangles.Add(row - 1);
                    triangles.Add(row);
                    triangles.Add(row + 1);
                }
            }
        }
    }
}
