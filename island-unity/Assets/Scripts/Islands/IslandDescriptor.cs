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
    internal float EstimatedBoundingRadiusMetres { get; }
    internal int GeneratorSchemaVersion { get; }

    internal IslandDescriptor(
        string islandId,
        Vector2Int worldCell,
        double logicalXMetres,
        double logicalZMetres,
        int seed,
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
        EstimatedBoundingRadiusMetres = estimatedBoundingRadiusMetres;
        GeneratorSchemaVersion = generatorSchemaVersion;
    }

    internal static IslandDescriptor Request(
        int seed,
        Vector2Int worldCell,
        string stableId)
    {
        var id = string.IsNullOrWhiteSpace(stableId)
            ? $"request-{seed}-cell-{worldCell.x}-{worldCell.y}"
            : stableId.Trim();
        var islandSizeMetres = IslandWorldManager.IslandSizeMetres;
        return new IslandDescriptor(
            id,
            worldCell,
            worldCell.x * (double)islandSizeMetres,
            worldCell.y * (double)islandSizeMetres,
            seed,
            islandSizeMetres * 0.5f,
            CurrentGeneratorSchemaVersion);
    }

    public bool Equals(IslandDescriptor other) => IslandId == other.IslandId;
    public override bool Equals(object value) =>
        value is IslandDescriptor other && Equals(other);
    public override int GetHashCode() => IslandId.GetHashCode();
    public override string ToString() => IslandId;
}
