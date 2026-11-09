using UnityEngine;

namespace Motu.World
{
    public sealed partial class TerrainFoundation
    {
        private bool TryMeasureContent(out Bounds bounds, out Quaternion rotation)
        {
            var content = FootprintRoot;
            rotation = Quaternion.Euler(0, content.eulerAngles.y, 0);
            var toFrame = Matrix4x4.Rotate(Quaternion.Inverse(rotation));
            bounds = default;
            var found = false;
            foreach (var renderer in content.GetComponentsInChildren<Renderer>())
            {
                if (!renderer.enabled || !(renderer is MeshRenderer || renderer is SkinnedMeshRenderer)
                    || IsGeneratedPart(renderer.transform)) continue;
                EncapsulateCorners(renderer.localBounds, toFrame * renderer.localToWorldMatrix, ref bounds, ref found);
            }
            if (!found)
                foreach (var collider in content.GetComponentsInChildren<Collider>())
                    if (collider.enabled && !collider.isTrigger && !IsGeneratedPart(collider.transform))
                        EncapsulateCorners(collider.bounds, toFrame, ref bounds, ref found);
            if (!found || bounds.size.x <= 0 || bounds.size.z <= 0) return false;
            // X/Z dimensions are in the yaw frame; the centre is returned in world space.
            bounds.center = rotation * bounds.center;
            return Finite(bounds.center.x) && Finite(bounds.center.y) && Finite(bounds.center.z)
                && Finite(bounds.size.x) && Finite(bounds.size.y) && Finite(bounds.size.z);
        }

        private bool IsGeneratedPart(Transform candidate) => platform != null
            && (candidate == platform.transform || candidate.IsChildOf(platform.transform));

        private static void EncapsulateCorners(Bounds source, Matrix4x4 matrix, ref Bounds result, ref bool found)
        {
            for (var i = 0; i < 8; i++)
            {
                var corner = source.center + Vector3.Scale(source.extents,
                    new Vector3((i & 1) == 0 ? -1 : 1, (i & 2) == 0 ? -1 : 1, (i & 4) == 0 ? -1 : 1));
                var position = matrix.MultiplyPoint3x4(corner);
                if (found) result.Encapsulate(position);
                else { result = new Bounds(position, Vector3.zero); found = true; }
            }
        }

        private void WritePlatformMesh(Quaternion rotation, float bottom, float top)
        {
            var size = Footprint.Size;
            var extent = new Vector3(size.x * .5f, (top - bottom) * .5f, size.y * .5f);
            var centre = new Vector3(Footprint.Center.x, (top + bottom) * .5f, Footprint.Center.y);
            var toLocal = platform.transform.worldToLocalMatrix * Matrix4x4.TRS(centre, rotation, Vector3.one);
            var vertices = new Vector3[24]; var uv = new Vector2[24]; var triangles = new int[36];
            WriteFace(0, Vector3.up * extent.y, Vector3.right * extent.x, Vector3.back * extent.z, toLocal, vertices, uv);
            WriteFace(1, Vector3.down * extent.y, Vector3.right * extent.x, Vector3.forward * extent.z, toLocal, vertices, uv);
            WriteFace(2, Vector3.forward * extent.z, Vector3.right * extent.x, Vector3.up * extent.y, toLocal, vertices, uv);
            WriteFace(3, Vector3.back * extent.z, Vector3.left * extent.x, Vector3.up * extent.y, toLocal, vertices, uv);
            WriteFace(4, Vector3.right * extent.x, Vector3.back * extent.z, Vector3.up * extent.y, toLocal, vertices, uv);
            WriteFace(5, Vector3.left * extent.x, Vector3.forward * extent.z, Vector3.up * extent.y, toLocal, vertices, uv);
            bool mirrored = toLocal.determinant < 0;
            for (var face = 0; face < 6; face++)
            {
                int v = face * 4, t = face * 6;
                triangles[t] = v; triangles[t + 1] = v + (mirrored ? 2 : 1); triangles[t + 2] = v + (mirrored ? 1 : 2);
                triangles[t + 3] = v; triangles[t + 4] = v + (mirrored ? 3 : 2); triangles[t + 5] = v + (mirrored ? 2 : 3);
            }
            platformMesh.Clear(); platformMesh.vertices = vertices; platformMesh.uv = uv; platformMesh.triangles = triangles;
            platformMesh.RecalculateNormals(); platformMesh.RecalculateTangents(); platformMesh.RecalculateBounds();
        }

        private void WriteFace(int face, Vector3 centre, Vector3 horizontal, Vector3 vertical,
            Matrix4x4 toLocal, Vector3[] vertices, Vector2[] uv)
        {
            var repeat = new Vector2(horizontal.magnitude * 2 / textureTileSizeMetres.x,
                vertical.magnitude * 2 / textureTileSizeMetres.y);
            int start = face * 4;
            vertices[start] = toLocal.MultiplyPoint3x4(centre - horizontal - vertical);
            vertices[start + 1] = toLocal.MultiplyPoint3x4(centre + horizontal - vertical);
            vertices[start + 2] = toLocal.MultiplyPoint3x4(centre + horizontal + vertical);
            vertices[start + 3] = toLocal.MultiplyPoint3x4(centre - horizontal + vertical);
            uv[start] = Vector2.zero; uv[start + 1] = new Vector2(repeat.x, 0);
            uv[start + 2] = repeat; uv[start + 3] = new Vector2(0, repeat.y);
        }
    }
}
