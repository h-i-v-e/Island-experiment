using System.Collections.Generic;
using Motu.Rendering;
using Motu.Streaming;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Towns
{
    public sealed partial class TownRenderer
    {
        private const float FieldWallBaseWidth = .64f;
        private const float FieldWallBaseHalf = FieldWallBaseWidth * .5f;
        private const float FieldWallTopHalf = FieldWallBaseHalf * .69f;
        private const float FieldWallBurial = .14f;
        private const float FieldWallHeight = 1.02f;
        private const float FieldTileSize = NativeWorldSizeMetres / TerrainTileStreamer.Lod1Resolution;
        private static readonly Vector3 FieldWorldOffset = new Vector3(NativeWorldSizeMetres * .5f, 0f, NativeWorldSizeMetres * .5f);
        private readonly List<MeshCollider> roadsideWallColliders = new List<MeshCollider>();
        private readonly List<(Vector3 centre, Quaternion rotation, Vector3 size)> roadsideBoundaryNavigation =
            new List<(Vector3 centre, Quaternion rotation, Vector3 size)>();
        private TownRoadsideFieldPlan roadsideFields;
        private SharedNoiseTexture shrubNoise;

        /// <summary>Parcels established alongside completed cart tracks.</summary>
        public int RoadsideFieldCount => roadsideFields?.fields?.Length ?? 0;
        internal IReadOnlyList<MeshCollider> RoadsideWallColliders => roadsideWallColliders;
        internal IReadOnlyList<(Vector3 centre, Quaternion rotation, Vector3 size)> RoadsideBoundaryNavigation => roadsideBoundaryNavigation;

        private void BuildRoadsideFields(TownRoadsideFieldPlan plan, Material churchStone)
        {
            roadsideFields = plan;
            if (RoadsideFieldCount == 0) return;

            var tiles = new Dictionary<Vector2Int, FieldTile>();
            BuildFieldWalls(tiles, plan.walls);

            foreach (var gate in plan.gates)
                BuildFieldGate(tiles, FieldPosition(gate.left), FieldPosition(gate.right));

            var stone = churchStone ?? CreateMaterial(TownNative.ChurchStone, "field wall stone");
            var timber = CreatePierTimber();
            var bushes = BuildFieldBushes(tiles, plan.bushes);
            var root = new GameObject("Roadside fields").transform;
            root.SetParent(transform, false);
            foreach (var pair in tiles)
                BuildFieldTile(root, pair.Key, pair.Value, stone, timber, bushes);
        }

        private static Vector3 FieldPosition(float[] source, int offset = 0) => Vector(source, offset) - FieldWorldOffset;
        private static Vector3 FieldSide(Vector3 direction) => new Vector3(-direction.z, 0f, direction.x);

        private static FieldTile GetFieldTile(Dictionary<Vector2Int, FieldTile> tiles, Vector3 position)
        {
            var key = new Vector2Int(Mathf.FloorToInt((position.x + FieldWorldOffset.x) / FieldTileSize),
                Mathf.FloorToInt((position.z + FieldWorldOffset.z) / FieldTileSize));
            if (tiles.TryGetValue(key, out var tile)) return tile;
            var origin = new Vector3((key.x + .5f) * FieldTileSize - FieldWorldOffset.x, position.y,
                (key.y + .5f) * FieldTileSize - FieldWorldOffset.z);
            tile = new FieldTile(origin);
            tiles.Add(key, tile);
            return tile;
        }

        private void BuildFieldTile(Transform root, Vector2Int key, FieldTile tile, Material stone, Material timber,
            Material bushes)
        {
            var child = new GameObject($"Field tile {key.x}, {key.y}");
            child.transform.SetParent(root, false);
            child.transform.localPosition = tile.Origin;

            var renderers = new List<Renderer>(3);
            if (!tile.Stone.IsEmpty)
            {
                var mesh = Own(tile.Stone.CreateMesh("Stone walls"));
                renderers.Add(AddMesh(child.transform, mesh, stone, mesh.name).GetComponent<Renderer>());
                var collider = child.AddComponent<MeshCollider>();
                collider.sharedMesh = mesh;
                roadsideWallColliders.Add(collider);
            }
            if (!tile.Timber.IsEmpty)
            {
                var mesh = Own(tile.Timber.CreateMesh("Timber gates"));
                renderers.Add(AddMesh(child.transform, mesh, timber, mesh.name).GetComponent<Renderer>());
                foreach (var gate in tile.Gates)
                {
                    var gateRoot = new GameObject("Closed field gate");
                    gateRoot.transform.SetParent(child.transform, false);
                    gateRoot.transform.SetLocalPositionAndRotation(gate.centre - tile.Origin, gate.rotation);
                    gateRoot.AddComponent<BoxCollider>().size = gate.size;
                }
                mesh.UploadMeshData(true);
            }
            var group = child.AddComponent<LODGroup>();
            var boundaries = new LOD(.025f, renderers.ToArray());
            if (tile.Bushes != null)
            {
                var mesh = Own(tile.Bushes.CreateMesh("Windbreak bushes"));
                renderers.Add(AddMesh(child.transform, mesh, bushes, mesh.name).GetComponent<Renderer>());
                mesh.UploadMeshData(true);
                var planted = new LOD(.08f, renderers.ToArray());

                var branches = Own(tile.Bushes.CreateBranchMesh("Windbreak branches"));
                renderers.Add(AddMesh(child.transform, branches, bushes, branches.name).GetComponent<Renderer>());
                branches.UploadMeshData(true);
                var nearby = new LOD(BushMeshBuilder.BranchLodHeight, renderers.ToArray());
                group.SetLODs(boundaries.renderers.Length == 0
                    ? new[] { nearby, planted }
                    : new[] { nearby, planted, boundaries });
            }
            else
            {
                group.SetLODs(new[] { boundaries });
            }
            group.RecalculateBounds();
        }

        private Material BuildFieldBushes(Dictionary<Vector2Int, FieldTile> tiles, TownRoadsidePlanting[] bushes)
        {
            if (bushes == null || bushes.Length == 0) return null;
            foreach (var bush in bushes)
            {
                var root = FieldPosition(bush.position);
                var tile = GetFieldTile(tiles, root);
                tile.Bushes ??= new BushMeshBuilder();
                tile.Bushes.AddBush(root - tile.Origin, -bush.yaw * Mathf.Rad2Deg, bush.scale);
            }

            shrubNoise = SharedNoiseTexture.Acquire(SharedNoiseTexture.Kind.Cliff);
            return Own(BushMeshBuilder.CreateMaterial("Field windbreak bushes", shrubNoise.Texture));
        }

        private void BuildFieldWalls(Dictionary<Vector2Int, FieldTile> tiles, TownRoadsideWall[] walls)
        {
            var network = new FieldWallNetwork(walls);
            foreach (var segment in network.Segments)
            {
                var start = segment.Start.Node.Position;
                var end = segment.End.Node.Position;
                var middle = (start + end) * .5f;
                var tile = GetFieldTile(tiles, middle);
                var u = Vector3.Dot(start, segment.Start.Direction) * .5f;
                tile.Stone.AddWallProfile(start - tile.Origin, end - tile.Origin,
                    segment.Start, segment.End, u, segment.Start.Length);

                var height = Mathf.Abs(end.y - start.y) + FieldWallHeight + FieldWallBurial;
                var centre = middle + Vector3.up * ((FieldWallHeight - FieldWallBurial) * .5f);
                roadsideBoundaryNavigation.Add((centre, Quaternion.LookRotation(segment.Start.Direction),
                    new Vector3(FieldWallBaseWidth, height, segment.Start.Length)));
            }
            foreach (var junction in network.Junctions)
            {
                var tile = GetFieldTile(tiles, junction.Position);
                tile.Stone.AddWallJunction(junction.Position - tile.Origin, junction.Ends);
            }
        }

        private static Vector3 FieldDirection(Vector3 from, Vector3 to)
        {
            var difference = to - from;
            difference.y = 0f;
            return difference.normalized;
        }

        private void BuildFieldGate(Dictionary<Vector2Int, FieldTile> tiles, Vector3 left, Vector3 right)
        {
            var midpoint = (left + right) * .5f;
            var across = FieldDirection(left, right);
            var span = Vector3.Dot(right - left, across);
            if (span <= .3f) return;

            var tile = GetFieldTile(tiles, midpoint);
            tile.Timber.AddGate(left - tile.Origin, right - tile.Origin, across);
            var boundary = (midpoint + Vector3.up * .52f, Quaternion.LookRotation(across),
                new Vector3(.20f, 1.19f + Mathf.Abs(right.y - left.y), span + .20f));
            tile.Gates.Add(boundary);
            roadsideBoundaryNavigation.Add(boundary);
        }

        private void OnDrawGizmosSelected()
        {
            if (RoadsideFieldCount == 0) return;
            var matrix = Gizmos.matrix;
            var colour = Gizmos.color;
            Gizmos.matrix = transform.localToWorldMatrix;
            foreach (var field in roadsideFields.fields)
            {
                Gizmos.color = field.wheat ? new Color(1f, .82f, .19f) : new Color(.38f, .94f, .56f);
                for (var i = 0; i < field.outline.Length; i += 3)
                {
                    var a = FieldPosition(field.outline, i) + Vector3.up * .065f;
                    var b = FieldPosition(field.outline, (i + 3) % field.outline.Length) + Vector3.up * .065f;
                    Gizmos.DrawLine(a, b);
                }
            }
            Gizmos.color = Color.cyan;
            foreach (var gate in roadsideFields.gates)
            {
                var left = FieldPosition(gate.left) + Vector3.up * .18f;
                var right = FieldPosition(gate.right) + Vector3.up * .18f;
                Gizmos.DrawLine(left, right);
                Gizmos.DrawWireSphere(left, .20f);
                Gizmos.DrawWireSphere(right, .20f);
            }
            Gizmos.matrix = matrix;
            Gizmos.color = colour;
        }

        private sealed class FieldTile
        {
            public readonly Vector3 Origin;
            public readonly FieldMesh Stone = new FieldMesh();
            public readonly FieldMesh Timber = new FieldMesh();
            public BushMeshBuilder Bushes;
            public readonly List<(Vector3 centre, Quaternion rotation, Vector3 size)> Gates = new();

            public FieldTile(Vector3 origin) { Origin = origin; }
        }

        private sealed class FieldMesh
        {
            private readonly List<Vector3> positions = new List<Vector3>();
            private readonly List<Vector3> normals = new List<Vector3>();
            private readonly List<Vector2> uvs = new List<Vector2>();
            private readonly List<int> triangles = new List<int>();
            public bool IsEmpty => triangles.Count == 0;

            public Mesh CreateMesh(string name)
            {
                var mesh = new Mesh { name = name, indexFormat = positions.Count > 65535 ? IndexFormat.UInt32 : IndexFormat.UInt16 };
                mesh.SetVertices(positions);
                mesh.SetNormals(normals);
                mesh.SetUVs(0, uvs);
                mesh.SetTriangles(triangles, 0);
                return mesh;
            }

            public void AddWallProfile(Vector3 a, Vector3 b, WallEnd start, WallEnd end, float u, float length)
            {
                var baseA = a - Vector3.up * FieldWallBurial;
                var baseB = b - Vector3.up * FieldWallBurial;
                var topA = a + Vector3.up * FieldWallHeight;
                var topB = b + Vector3.up * FieldWallHeight;
                var lowLeftA = baseA + start.Clockwise * FieldWallBaseHalf;
                var lowRightA = baseA + start.Counterclockwise * FieldWallBaseHalf;
                var lowLeftB = baseB + end.Counterclockwise * FieldWallBaseHalf;
                var lowRightB = baseB + end.Clockwise * FieldWallBaseHalf;
                var highLeftA = topA + start.Clockwise * FieldWallTopHalf;
                var highRightA = topA + start.Counterclockwise * FieldWallTopHalf;
                var highLeftB = topB + end.Counterclockwise * FieldWallTopHalf;
                var highRightB = topB + end.Clockwise * FieldWallTopHalf;
                var surfaceLength = length * .5f;
                var surfaceHeight = (FieldWallHeight + FieldWallBurial) * .5f;
                AddQuad(lowRightA, lowRightB, highRightB, highRightA, u, surfaceLength, surfaceHeight);
                AddQuad(lowLeftB, lowLeftA, highLeftA, highLeftB, -u - surfaceLength, surfaceLength, surfaceHeight);
                AddQuad(highRightA, highRightB, highLeftB, highLeftA, u, surfaceLength, FieldWallTopHalf);
                if (start.Node.Ends.Count == 1)
                    AddQuad(lowLeftA, lowRightA, highRightA, highLeftA, u, FieldWallBaseHalf, surfaceHeight);
                if (end.Node.Ends.Count == 1)
                    AddQuad(lowRightB, lowLeftB, highLeftB, highRightB, u, FieldWallBaseHalf, surfaceHeight);
            }

            public void AddWallJunction(Vector3 centre, IReadOnlyList<WallEnd> ends)
            {
                centre.y += FieldWallHeight;
                var index = positions.Count;
                AddRoofVertex(centre);
                foreach (var end in ends)
                    AddRoofVertex(centre + end.Counterclockwise * FieldWallTopHalf);
                for (var i = 0; i < ends.Count; ++i)
                    AddIndices(index, index + 1 + (i + 1) % ends.Count, index + 1 + i);

                void AddRoofVertex(Vector3 point) =>
                    AddVertex(point, Vector3.up, new Vector2(point.x, point.z) * .5f);
            }

            public void AddGate(Vector3 left, Vector3 right, Vector3 across)
            {
                var normal = FieldSide(across);
                AddBeam(left - Vector3.up * .16f, left + Vector3.up * 1.12f, normal, .20f, .20f);
                AddBeam(right - Vector3.up * .16f, right + Vector3.up * 1.12f, normal, .20f, .20f);

                var start = left + across * .12f;
                var end = right - across * .12f;
                AddBeam(start + Vector3.up * .23f, start + Vector3.up * .98f, normal, .10f, .09f);
                AddBeam(end + Vector3.up * .23f, end + Vector3.up * .98f, normal, .10f, .09f);
                for (var rail = 0; rail < 4; ++rail)
                {
                    var height = Vector3.up * (.28f + rail * .22f);
                    AddBeam(start + height, end + height, normal, .09f, .065f);
                }
                AddBeam(start + Vector3.up * .30f, end + Vector3.up * .92f, normal, .075f, .09f);
            }

            public void AddBeam(Vector3 a, Vector3 b, Vector3 depthAxis, float width, float depth)
            {
                var along = b - a;
                var w = Vector3.Cross(depthAxis, along).normalized * (width * .5f);
                var d = depthAxis * (depth * .5f);
                var length = along.magnitude;
                AddQuad(a - w + d, b - w + d, b + w + d, a + w + d, 0f, length, width);
                AddQuad(b - w - d, a - w - d, a + w - d, b + w - d, 0f, length, width);
                AddQuad(a + w + d, b + w + d, b + w - d, a + w - d, 0f, length, depth);
                AddQuad(b - w + d, a - w + d, a - w - d, b - w - d, 0f, length, depth);
                AddQuad(a - w - d, a - w + d, a + w + d, a + w - d, 0f, depth, width);
                AddQuad(b - w + d, b - w - d, b + w - d, b + w + d, 0f, depth, width);
            }

            private void AddQuad(Vector3 a, Vector3 b, Vector3 c, Vector3 d, float u, float width, float height)
            {
                var index = positions.Count;
                var normal = Vector3.Cross(b - a, c - a).normalized;
                AddVertex(a, normal, new Vector2(u, 0f));
                AddVertex(b, normal, new Vector2(u + width, 0f));
                AddVertex(c, normal, new Vector2(u + width, height));
                AddVertex(d, normal, new Vector2(u, height));
                AddIndices(index, index + 1, index + 2);
                AddIndices(index, index + 2, index + 3);
            }

            private void AddVertex(Vector3 position, Vector3 normal, Vector2 uv)
            {
                positions.Add(position);
                normals.Add(normal);
                uvs.Add(uv);
            }

            private void AddIndices(int a, int b, int c)
            {
                triangles.Add(a);
                triangles.Add(b);
                triangles.Add(c);
            }
        }
    }
}
