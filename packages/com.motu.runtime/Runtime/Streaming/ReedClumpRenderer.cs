using Motu.Islands;
using UnityEngine;
using UnityEngine.Rendering;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Streaming
{
    // Every island draws the same 32-vertex clump; only nearby placement batches
    // acquire matrix/property arrays. Bounds never span more than one owner tile.
    internal static class ReedClumpMesh
    {
        private static Mesh shared;
        private static int users;

        internal static Mesh Acquire()
        {
            if (shared == null) shared = Create();
            ++users;
            return shared;
        }

        internal static void Release()
        {
            if (users == 0 || --users != 0) return;
            DestroyUnityObject(shared);
            shared = null;
        }

        private static Mesh Create()
        {
            const int planes = 4, levels = 4;
            var vertices = new Vector3[planes * levels * 2];
            var normals = new Vector3[vertices.Length];
            var uv = new Vector2[vertices.Length];
            var triangles = new int[planes * (levels - 1) * 6];
            var triangle = 0;
            for (var plane = 0; plane < planes; ++plane)
            {
                var angle = plane * Mathf.PI * .25f;
                var tangent = new Vector3(Mathf.Cos(angle), 0, Mathf.Sin(angle));
                var normal = new Vector3(-tangent.z, 0, tangent.x);
                for (var level = 0; level < levels; ++level)
                {
                    var height = level / (float)(levels - 1);
                    var row = plane * levels * 2 + level * 2;
                    for (var side = 0; side < 2; ++side)
                    {
                        vertices[row + side] = tangent * (side - .5f) + Vector3.up * height;
                        normals[row + side] = normal;
                        uv[row + side] = new Vector2(side, height);
                    }
                    if (level == levels - 1) continue;
                    // Match the legacy mesh export's native Z-up to Unity Y-up winding flip.
                    triangles[triangle++] = row;
                    triangles[triangle++] = row + 2;
                    triangles[triangle++] = row + 1;
                    triangles[triangle++] = row + 1;
                    triangles[triangle++] = row + 2;
                    triangles[triangle++] = row + 3;
                }
            }
            var mesh = new Mesh { name = "Shared riverbank reed and rush clump",
                vertices = vertices, normals = normals, uv = uv, triangles = triangles };
            mesh.RecalculateBounds();
            mesh.UploadMeshData(false);
            return mesh;
        }
    }

    internal sealed class ReedClumpBatch
    {
        internal const int Limit = 128;
        internal static readonly int DataId = Shader.PropertyToID("_ReedInstanceData");
        internal static readonly int GroundId = Shader.PropertyToID("_ReedGround");
        private readonly Matrix4x4[] local;
        private readonly Matrix4x4[] world;
        private readonly Vector4[] data;
        private readonly Vector4[] ground;
        private readonly MaterialPropertyBlock properties = new MaterialPropertyBlock();
        private readonly MaterialPropertyBlock fallback = new MaterialPropertyBlock();
        private Bounds localBounds;
        private Bounds worldBounds;
        internal int Count => local.Length;

        internal ReedClumpBatch(IslandPreparedReedPlacement[] placements, int offset, int count)
        {
            local = new Matrix4x4[count];
            world = new Matrix4x4[count];
            data = new Vector4[count];
            ground = new Vector4[count];
            for (var i = 0; i < count; ++i)
            {
                var placement = placements[offset + i];
                var width = placement.widthMetres;
                var height = placement.heightMetres;
                local[i] = Matrix4x4.TRS(placement.localRoot, Quaternion.identity,
                    new Vector3(width, height, width));
                data[i] = placement.data;
                var normal = placement.groundNormal;
                var up = Mathf.Abs(normal.y) > .00001f ? normal.y : 1f;
                ground[i] = new Vector4(-normal.x / up * width / height,
                    -normal.z / up * width / height, 0, 0);
                var groundExtent = width * .5f * (Mathf.Abs(normal.x) + Mathf.Abs(normal.z)) / Mathf.Abs(up);
                var minimum = placement.localRoot - new Vector3(width * .5f, groundExtent, width * .5f);
                var maximum = placement.localRoot + new Vector3(width * .5f, Mathf.Max(height, groundExtent), width * .5f);
                if (i == 0) localBounds = new Bounds((minimum + maximum) * .5f, maximum - minimum);
                else { localBounds.Encapsulate(minimum); localBounds.Encapsulate(maximum); }
            }
            properties.SetVectorArray(DataId, data);
            properties.SetVectorArray(GroundId, ground);
        }

        internal void SetTransform(Matrix4x4 matrix)
        {
            for (var i = 0; i < world.Length; ++i) world[i] = matrix * local[i];
            worldBounds = TransformBounds(matrix, localBounds);
        }

        internal void Submit(Camera camera, Mesh mesh, Material material, int layer, float windPadding)
        {
            var bounds = worldBounds;
            bounds.Expand(new Vector3(windPadding * 2, .1f, windPadding * 2));
            if (SystemInfo.supportsInstancing)
            {
                var parameters = new RenderParams(material) { camera = camera, worldBounds = bounds,
                    shadowCastingMode = ShadowCastingMode.On, receiveShadows = true,
                    matProps = properties, layer = layer, lightProbeUsage = LightProbeUsage.Off };
                Graphics.RenderMeshInstanced(parameters, mesh, 0, world, Count);
                return;
            }
            for (var i = 0; i < Count; ++i)
            {
                fallback.SetVector(DataId, data[i]);
                fallback.SetVector(GroundId, ground[i]);
                Graphics.DrawMesh(mesh, world[i], material, layer, camera, 0, fallback,
                    ShadowCastingMode.On, true, null, LightProbeUsage.Off);
            }
        }

        private static Bounds TransformBounds(Matrix4x4 matrix, Bounds bounds)
        {
            var x = matrix.MultiplyVector(new Vector3(bounds.extents.x, 0, 0));
            var y = matrix.MultiplyVector(new Vector3(0, bounds.extents.y, 0));
            var z = matrix.MultiplyVector(new Vector3(0, 0, bounds.extents.z));
            var extent = new Vector3(Mathf.Abs(x.x) + Mathf.Abs(y.x) + Mathf.Abs(z.x),
                Mathf.Abs(x.y) + Mathf.Abs(y.y) + Mathf.Abs(z.y),
                Mathf.Abs(x.z) + Mathf.Abs(y.z) + Mathf.Abs(z.z));
            return new Bounds(matrix.MultiplyPoint3x4(bounds.center), extent * 2);
        }
    }
}
