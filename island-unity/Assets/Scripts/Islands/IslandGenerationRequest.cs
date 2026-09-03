using System;

internal sealed class IslandGenerationRequest
{
    internal IslandDescriptor Descriptor { get; }
    internal MotuNative.Options Options { get; }
    internal MotuNative.ForestOptions ForestOptions { get; }
    internal MotuNative.ReedOptions ReedOptions { get; }
    internal MotuNative.FernOptions FernOptions { get; }
    internal float WorldSizeMetres { get; }
    internal IslandMaterialColours MaterialColours { get; }
    internal int MaterialTextureResolution { get; }
    internal string SnapshotPath { get; }
    internal long SnapshotCacheBudgetBytes { get; }

    internal IslandGenerationRequest(
        IslandDescriptor descriptor,
        MotuNative.Options options,
        MotuNative.ForestOptions forestOptions,
        MotuNative.ReedOptions reedOptions,
        MotuNative.FernOptions fernOptions,
        float worldSizeMetres,
        IslandMaterialColours materialColours,
        int materialTextureResolution,
        bool useSnapshotCache = true,
        long snapshotCacheBudgetBytes = 8L * 1024L * 1024L * 1024L)
    {
        if (string.IsNullOrWhiteSpace(descriptor.IslandId))
        {
            throw new ArgumentException(
                "An island generation request requires a valid descriptor.",
                nameof(descriptor));
        }
        if (float.IsNaN(worldSizeMetres)
            || float.IsInfinity(worldSizeMetres)
            || worldSizeMetres <= 0f)
        {
            throw new ArgumentOutOfRangeException(nameof(worldSizeMetres));
        }
        if (materialTextureResolution <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(materialTextureResolution));
        }
        Descriptor = descriptor;
        Options = options;
        ForestOptions = forestOptions;
        ReedOptions = reedOptions;
        FernOptions = fernOptions;
        WorldSizeMetres = worldSizeMetres;
        MaterialColours = materialColours;
        MaterialTextureResolution = materialTextureResolution;
        SnapshotCacheBudgetBytes = Math.Max(snapshotCacheBudgetBytes, 0L);
        SnapshotPath = useSnapshotCache ? IslandSnapshotCache.PathFor(this) : null;
    }
}
