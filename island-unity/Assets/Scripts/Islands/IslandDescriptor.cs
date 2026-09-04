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
            worldSizeMetres * 0.5f,
            CurrentGeneratorSchemaVersion);
    }

    internal static IslandDescriptor Authored(
        string islandId,
        Vector2Int worldCell,
        IslandGenerator generator,
        float cellSizeMetres)
    {
        if (generator == null) throw new ArgumentNullException(nameof(generator));
        if (!IsFinitePositive(cellSizeMetres))
        {
            throw new ArgumentOutOfRangeException(
                nameof(cellSizeMetres),
                "Island cell size must be finite and positive.");
        }
        return new IslandDescriptor(
            islandId,
            worldCell,
            worldCell.x * (double)cellSizeMetres,
            worldCell.y * (double)cellSizeMetres,
            generator.Generation.Seed,
            generator.WorldSizeMetres * 0.5f,
            CurrentGeneratorSchemaVersion);
    }

    internal static bool TryCreateProcedural(
        int worldSeed,
        Vector2Int worldCell,
        float cellSizeMetres,
        float occupancy,
        out IslandDescriptor descriptor)
    {
        if (!IsFinitePositive(cellSizeMetres))
        {
            throw new ArgumentOutOfRangeException(
                nameof(cellSizeMetres),
                "Procedural island cell size must be finite and positive.");
        }

        var occupancyHash = Hash(worldSeed, worldCell.x, worldCell.y, 0x4f1bbcdcu);
        if (HashUnitFloat(occupancyHash) >= Mathf.Clamp01(occupancy))
        {
            descriptor = default;
            return false;
        }

        var seedHash = Hash(worldSeed, worldCell.x, worldCell.y, 0x27d4eb2fu);
        var islandSeed = (int)(seedHash & 0x7fffffffu);
        if (islandSeed == 0)
        {
            islandSeed = 1;
        }
        descriptor = new IslandDescriptor(
            $"world-{worldSeed}-cell-{worldCell.x}-{worldCell.y}",
            worldCell,
            worldCell.x * (double)cellSizeMetres,
            worldCell.y * (double)cellSizeMetres,
            islandSeed,
            cellSizeMetres * 0.5f,
            CurrentGeneratorSchemaVersion);
        return true;
    }

    private static uint Hash(
        int worldSeed,
        int cellX,
        int cellZ,
        uint salt)
    {
        unchecked
        {
            var value = (uint)worldSeed ^ salt;
            value ^= (uint)cellX * 0x9e3779b9u;
            value = (value << 17) | (value >> 15);
            value ^= (uint)cellZ * 0x85ebca6bu;
            value ^= value >> 16;
            value *= 0x7feb352du;
            value ^= value >> 15;
            value *= 0x846ca68bu;
            value ^= value >> 16;
            return value;
        }
    }

    private static float HashUnitFloat(uint value) =>
        (value & 0x00ffffffu) / 16777216f;

    private static bool IsFinitePositive(float value) =>
        !float.IsNaN(value) && !float.IsInfinity(value) && value > 0f;

    public bool Equals(IslandDescriptor other) => IslandId == other.IslandId;
    public override bool Equals(object value) =>
        value is IslandDescriptor other && Equals(other);
    public override int GetHashCode() => IslandId.GetHashCode();
    public override string ToString() => IslandId;
}
