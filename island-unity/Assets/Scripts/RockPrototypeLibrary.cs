using System;
using UnityEngine;

internal sealed class RockPrototypeLibrary : IDisposable
{
    internal const int StoneCount = 12;
    internal const int BoulderCount = 8;
    internal const int PrototypeCount = StoneCount + BoulderCount;
    internal const int ExpectedVertexCount = 12;
    internal const int ExpectedTriangleCount = 20;
    internal const float StoneMinimumDiameter = 0.10f;
    internal const float StoneMaximumDiameter = 0.30f;
    internal const float BoulderMinimumDiameter = 0.30f;
    internal const float BoulderMaximumDiameter = 0.60f;
    internal const float FlatSurfaceEmbedRatio = 0.10f;
    internal const float MaximumSlopeEmbedRatio = 0.50f;
    internal const float MaximumSettledSlopeDegrees = 25f;

    private const uint PrototypeSeed = 0x91E10DA5u;

    private sealed class Prototype
    {
        internal readonly Mesh mesh;
        internal readonly Vector3[] vertices;

        internal Prototype(Mesh mesh, Vector3[] vertices)
        {
            this.mesh = mesh;
            this.vertices = vertices;
        }
    }

    private struct DeterministicRandom
    {
        private uint state;

        internal DeterministicRandom(uint seed)
        {
            state = seed == 0 ? 0xA341316Cu : seed;
        }

        internal uint NextUInt()
        {
            var value = state;
            value ^= value << 13;
            value ^= value >> 17;
            value ^= value << 5;
            state = value;
            return value;
        }

        internal float Next01()
        {
            return (NextUInt() >> 8) * (1f / 16777216f);
        }

        internal float Range(float minimum, float maximum)
        {
            return Mathf.Lerp(minimum, maximum, Next01());
        }

        internal Vector3 UnitVector()
        {
            var z = Range(-1f, 1f);
            var angle = Range(0f, Mathf.PI * 2f);
            var radius = Mathf.Sqrt(Mathf.Max(0f, 1f - z * z));
            return new Vector3(radius * Mathf.Cos(angle), z, radius * Mathf.Sin(angle));
        }
    }

    private readonly Prototype[] prototypes = new Prototype[PrototypeCount];
    private bool disposed;

    internal RockPrototypeLibrary()
    {
        for (var index = 0; index < prototypes.Length; index++)
        {
            prototypes[index] = CreatePrototype(index, index >= StoneCount);
        }
    }

    internal int Count => prototypes.Length;

    internal Mesh MeshAt(int prototypeIndex)
    {
        ValidateIndex(prototypeIndex);
        return prototypes[prototypeIndex].mesh;
    }

    internal static float EmbedRatioForNormal(Vector3 normal)
    {
        var slopeDegrees = Mathf.Acos(Mathf.Clamp(normal.y, -1f, 1f)) * Mathf.Rad2Deg;
        var slopeAmount = Mathf.Clamp01(slopeDegrees / MaximumSettledSlopeDegrees);
        return Mathf.Lerp(FlatSurfaceEmbedRatio, MaximumSlopeEmbedRatio, slopeAmount);
    }

    internal Vector3 SeatPosition(IslandPreparedRockDecoration candidate)
    {
        ValidateIndex(candidate.prototypeIndex);
        var support = float.PositiveInfinity;
        var vertices = prototypes[candidate.prototypeIndex].vertices;
        for (var index = 0; index < vertices.Length; index++)
        {
            var local = Vector3.Scale(vertices[index], candidate.scale);
            var worldOffset = candidate.rotation * local;
            support = Mathf.Min(support, Vector3.Dot(worldOffset, candidate.normal));
        }
        if (!IsFinite(support))
        {
            throw new InvalidOperationException("A rock prototype produced a non-finite support point.");
        }
        return candidate.position
            - candidate.normal * (support + candidate.embedDepth);
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }
        disposed = true;
        for (var index = 0; index < prototypes.Length; index++)
        {
            if (prototypes[index] != null)
            {
                DestroyUnityObject(prototypes[index].mesh);
                prototypes[index] = null;
            }
        }
    }

    private static Prototype CreatePrototype(int prototypeIndex, bool boulder)
    {
        BuildIcosahedron(out var unitVertices, out var triangles);
        var random = new DeterministicRandom(
            PrototypeSeed ^ ((uint)prototypeIndex + 1u) * 0x9E3779B9u);
        var axisScale = boulder
            ? new Vector3(random.Range(0.88f, 1.14f), random.Range(0.82f, 1.12f), random.Range(0.88f, 1.14f))
            : new Vector3(random.Range(0.90f, 1.12f), random.Range(0.78f, 1.04f), random.Range(0.90f, 1.12f));
        var direction0 = random.UnitVector();
        var direction1 = random.UnitVector();
        var direction2 = random.UnitVector();
        var biasDirection = random.UnitVector();
        var phase0 = random.Range(0f, Mathf.PI * 2f);
        var phase1 = random.Range(0f, Mathf.PI * 2f);
        var phase2 = random.Range(0f, Mathf.PI * 2f);
        var vertices = new Vector3[unitVertices.Length];
        for (var index = 0; index < vertices.Length; index++)
        {
            var direction = unitVertices[index].normalized;
            var noise = Mathf.Sin(Vector3.Dot(direction, direction0) * Mathf.PI * 2f + phase0) * 0.11f
                + Mathf.Sin(Vector3.Dot(direction, direction1) * Mathf.PI * 4f + phase1) * 0.055f
                + Mathf.Sin(Vector3.Dot(direction, direction2) * Mathf.PI * 7f + phase2) * 0.025f
                + Vector3.Dot(direction, biasDirection) * 0.075f;
            var radius = Mathf.Clamp(1f + noise, 0.72f, 1.28f) * 0.5f;
            vertices[index] = Vector3.Scale(direction * radius, axisScale);
            if (!IsFinite(vertices[index]))
            {
                throw new InvalidOperationException("A generated rock prototype contains a non-finite vertex.");
            }
        }

        if (vertices.Length != ExpectedVertexCount
            || triangles.Length / 3 != ExpectedTriangleCount)
        {
            throw new InvalidOperationException("The generated rock prototype topology is invalid.");
        }

        var mesh = new Mesh
        {
            name = $"Rock prototype {prototypeIndex:00}"
        };
        mesh.vertices = vertices;
        mesh.triangles = triangles;
        mesh.RecalculateNormals();
        mesh.RecalculateBounds();
        if (mesh.bounds.size.sqrMagnitude <= 1.0e-8f)
        {
            DestroyUnityObject(mesh);
            throw new InvalidOperationException("A generated rock prototype has zero volume.");
        }
        return new Prototype(mesh, vertices);
    }

    private static void BuildIcosahedron(
        out Vector3[] vertices,
        out int[] triangles)
    {
        var t = (1f + Mathf.Sqrt(5f)) * 0.5f;
        vertices = new[]
        {
            new Vector3(-1, t, 0).normalized,
            new Vector3(1, t, 0).normalized,
            new Vector3(-1, -t, 0).normalized,
            new Vector3(1, -t, 0).normalized,
            new Vector3(0, -1, t).normalized,
            new Vector3(0, 1, t).normalized,
            new Vector3(0, -1, -t).normalized,
            new Vector3(0, 1, -t).normalized,
            new Vector3(t, 0, -1).normalized,
            new Vector3(t, 0, 1).normalized,
            new Vector3(-t, 0, -1).normalized,
            new Vector3(-t, 0, 1).normalized,
        };
        triangles = new[]
        {
            0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11,
            1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
            3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9,
            4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
        };
    }

    private static void ValidateIndex(int prototypeIndex)
    {
        if (prototypeIndex < 0 || prototypeIndex >= PrototypeCount)
        {
            throw new ArgumentOutOfRangeException(nameof(prototypeIndex));
        }
    }

    private static bool IsFinite(float value)
    {
        return !float.IsNaN(value) && !float.IsInfinity(value);
    }

    private static bool IsFinite(Vector3 value)
    {
        return IsFinite(value.x) && IsFinite(value.y) && IsFinite(value.z);
    }

    private static void DestroyUnityObject(UnityEngine.Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            UnityEngine.Object.Destroy(value);
        }
        else
        {
            UnityEngine.Object.DestroyImmediate(value);
        }
    }
}
