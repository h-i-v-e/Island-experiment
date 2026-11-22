using System.Collections.Generic;
using UnityEngine;

namespace Motu.Towns
{
    public sealed partial class TownRenderer
    {
        private sealed class FieldWallNetwork
        {
            private const float WeldTolerance = .002f;
            private const float BinSize = 4f;
            private static readonly Vector2 WeldMargin = Vector2.one * WeldTolerance;
            private readonly Dictionary<Vector2Int, List<WallNode>> bins = new();
            internal readonly List<(WallEnd Start, WallEnd End)> Segments = new();
            internal readonly List<WallNode> Junctions = new();

            internal FieldWallNetwork(TownRoadsideWall[] walls)
            {
                var runs = new List<(WallNode Start, WallNode End)>();
                foreach (var wall in walls)
                {
                    WallNode previous = null;
                    for (var offset = 0; offset + 2 < wall.points.Length; offset += 3)
                    {
                        var node = FindOrAdd(FieldPosition(wall.points, offset));
                        if (previous != null && previous != node) runs.Add((previous, node));
                        previous = node;
                    }
                }

                var splits = new List<(float Distance, WallNode Node)>();
                foreach (var (start, end) in runs)
                    AddRun(start, end, splits);
                foreach (var bin in bins.Values)
                    foreach (var node in bin)
                        FitJunction(node);
            }

            private static Vector2 Plan(Vector3 p) => new Vector2(p.x, p.z);
            private static Vector2Int Bin(Vector2 point) => Vector2Int.FloorToInt(point / BinSize);

            private WallNode FindOrAdd(Vector3 position)
            {
                var point = Plan(position);
                foreach (var node in Nearby(point, point))
                    if ((Plan(node.Position) - point).sqrMagnitude <= WeldTolerance * WeldTolerance) return node;

                var result = new WallNode(position);
                var key = Bin(point);
                if (!bins.TryGetValue(key, out var nodes)) bins.Add(key, nodes = new List<WallNode>());
                nodes.Add(result);
                return result;
            }

            private IEnumerable<WallNode> Nearby(Vector2 min, Vector2 max)
            {
                var lower = Bin(min - WeldMargin);
                var upper = Bin(max + WeldMargin);
                for (var y = lower.y; y <= upper.y; ++y)
                    for (var x = lower.x; x <= upper.x; ++x)
                        if (bins.TryGetValue(new Vector2Int(x, y), out var nodes))
                            foreach (var node in nodes)
                                yield return node;
            }

            private void AddRun(WallNode start, WallNode end, List<(float Distance, WallNode Node)> splits)
            {
                // A shorter parcel's back wall can end midway along its neighbour's divider.
                splits.Clear();
                var a = Plan(start.Position);
                var b = Plan(end.Position);
                var length = Vector2.Distance(a, b);
                var direction = (b - a) / length;
                foreach (var node in Nearby(Vector2.Min(a, b), Vector2.Max(a, b)))
                {
                    var delta = Plan(node.Position) - a;
                    var distance = Vector2.Dot(delta, direction);
                    if (distance <= WeldTolerance || distance >= length - WeldTolerance) continue;
                    if ((delta - direction * distance).sqrMagnitude <= WeldTolerance * WeldTolerance)
                        splits.Add((distance, node));
                }

                splits.Sort((left, right) => left.Distance.CompareTo(right.Distance));
                var previous = start;
                foreach (var (_, node) in splits)
                {
                    Connect(previous, node);
                    previous = node;
                }
                Connect(previous, end);
            }

            private void Connect(WallNode start, WallNode end)
            {
                if (start.Ends.Exists(edge => edge.Other == end)) return;
                var length = Vector2.Distance(Plan(start.Position), Plan(end.Position));
                var direction = FieldDirection(start.Position, end.Position);
                var outgoing = new WallEnd(start, end, direction, length);
                var incoming = new WallEnd(end, start, -direction, length);
                start.Ends.Add(outgoing);
                end.Ends.Add(incoming);
                Segments.Add((outgoing, incoming));
            }

            private void FitJunction(WallNode node)
            {
                var ends = node.Ends;
                if (ends.Count < 2) return;
                ends.Sort((left, right) => left.Bearing.CompareTo(right.Bearing));
                for (var i = 0; i < ends.Count; ++i)
                {
                    var a = ends[i];
                    var b = ends[(i + 1) % ends.Count];
                    // The outside edge of each neighbouring strip uses the same corner.
                    var divisor = 1f - Vector3.Dot(a.Direction, b.Direction);
                    var miter = divisor > .0001f
                        ? Vector3.ClampMagnitude((FieldSide(a.Direction) - FieldSide(b.Direction)) / divisor, 2.25f)
                        : Vector3.zero;
                    a.Counterclockwise = b.Clockwise = miter;
                }
                if (ends.Count > 2) Junctions.Add(node);
            }
        }

        private sealed class WallNode
        {
            internal readonly Vector3 Position;
            internal readonly List<WallEnd> Ends = new(2);

            internal WallNode(Vector3 position) => Position = position;
        }

        private sealed class WallEnd
        {
            internal readonly WallNode Node, Other;
            internal readonly Vector3 Direction;
            internal readonly float Bearing, Length;
            internal Vector3 Clockwise, Counterclockwise;

            internal WallEnd(WallNode node, WallNode other, Vector3 direction, float length)
            {
                Node = node;
                Other = other;
                Direction = direction;
                Bearing = Mathf.Atan2(direction.z, direction.x);
                Length = length;
                Counterclockwise = FieldSide(direction);
                Clockwise = -Counterclockwise;
            }
        }
    }
}
