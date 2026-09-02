using System;
using UnityEngine;

internal readonly struct IslandDescriptor : IEquatable<IslandDescriptor>
{
    private const int CurrentGeneratorSchemaVersion = 1;

    internal string IslandId { get; }
    internal Vector2Int WorldCell { get; }
    internal double LogicalXMetres { get; }
    internal double LogicalZMetres { get; }
    internal int Seed { get; }
    internal float RotationDegrees { get; }
    internal float EstimatedBoundingRadiusMetres { get; }
    internal int GeneratorSchemaVersion { get; }

    internal IslandDescriptor(
        string islandId,
        Vector2Int worldCell,
        double logicalXMetres,
        double logicalZMetres,
        int seed,
        float rotationDegrees,
        float estimatedBoundingRadiusMetres,
        int generatorSchemaVersion)
    {
        if (string.IsNullOrWhiteSpace(islandId))
        {
            throw new ArgumentException("An island descriptor requires a stable ID.", nameof(islandId));
        }
        if (double.IsNaN(logicalXMetres)
            || double.IsInfinity(logicalXMetres)
            || double.IsNaN(logicalZMetres)
            || double.IsInfinity(logicalZMetres)
            || float.IsNaN(rotationDegrees)
            || float.IsInfinity(rotationDegrees)
            || float.IsNaN(estimatedBoundingRadiusMetres)
            || float.IsInfinity(estimatedBoundingRadiusMetres)
            || estimatedBoundingRadiusMetres <= 0f)
        {
            throw new ArgumentOutOfRangeException(
                nameof(estimatedBoundingRadiusMetres),
                "Island descriptor coordinates and bounds must be finite and positive.");
        }
        IslandId = islandId;
        WorldCell = worldCell;
        LogicalXMetres = logicalXMetres;
        LogicalZMetres = logicalZMetres;
        Seed = seed;
        RotationDegrees = rotationDegrees;
        EstimatedBoundingRadiusMetres = estimatedBoundingRadiusMetres;
        GeneratorSchemaVersion = generatorSchemaVersion;
    }

    internal static IslandDescriptor Origin(
        int seed,
        float worldSizeMetres,
        Transform anchor)
    {
        if (anchor == null) throw new ArgumentNullException(nameof(anchor));
        return new IslandDescriptor(
            $"origin-{seed}",
            Vector2Int.zero,
            anchor.position.x,
            anchor.position.z,
            seed,
            anchor.eulerAngles.y,
            worldSizeMetres * 0.5f,
            CurrentGeneratorSchemaVersion);
    }

    internal static IslandDescriptor Authored(
        string islandId,
        Vector2Int worldCell,
        IslandGenerator generator)
    {
        if (generator == null) throw new ArgumentNullException(nameof(generator));
        return new IslandDescriptor(
            islandId,
            worldCell,
            generator.transform.position.x,
            generator.transform.position.z,
            generator.Generation.Seed,
            generator.transform.eulerAngles.y,
            generator.WorldSizeMetres * 0.5f,
            CurrentGeneratorSchemaVersion);
    }

    public bool Equals(IslandDescriptor other) => IslandId == other.IslandId;
    public override bool Equals(object value) =>
        value is IslandDescriptor other && Equals(other);
    public override int GetHashCode() => IslandId.GetHashCode();
    public override string ToString() => IslandId;
}
