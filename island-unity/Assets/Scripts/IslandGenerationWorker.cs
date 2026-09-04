using System.Threading;
using System.Threading.Tasks;

internal static class IslandGenerationWorker
{
    internal static Task<IslandPreparedData> GenerateAsync(
        IslandGenerationRequest request,
        CancellationToken cancellationToken)
    {
        if (request == null) throw new System.ArgumentNullException(nameof(request));
        return Task.Run(
            () => IslandPreparationPipeline.PrepareIsland(
                request,
                cancellationToken),
            cancellationToken);
    }

    internal static Task<IslandPreparedData> GenerateAsync(
        int seed,
        MotuNative.Options options,
        float worldSize,
        IslandMaterialColours materialColours,
        int materialTextureResolution,
        CancellationToken cancellationToken)
    {
        return GenerateAsync(
            seed,
            options,
            new MotuNative.ForestOptions
            {
                patchSizeMetres = 200f * 2000f / worldSize,
                noiseThreshold = 0.62f,
                noiseOctaves = 4,
                snowlineMetres = 100f * 2000f / worldSize,
                prototypeCount = 8,
                minimumScale = 1f,
                maximumScale = 2f,
            },
            new MotuNative.ReedOptions
            {
                bankWidthMetres = 2.5f * 2000f / worldSize,
                patchSizeMetres = 12f * 2000f / worldSize,
                coverageThreshold = 0.38f,
                spacingMetres = 0.75f * 2000f / worldSize,
                rushRatio = 0.45f,
                minimumHeightMetres = 0.65f * 2000f / worldSize,
                maximumHeightMetres = 2.1f * 2000f / worldSize,
                maximumSlopeDegrees = 32f,
            },
            new MotuNative.FernOptions
            {
                barkClearanceMetres = 0.18f * 2000f / worldSize,
                outerRadiusMetres = 1.65f * 2000f / worldSize,
                spacingMetres = 0.58f * 2000f / worldSize,
                patchSizeMetres = 12f * 2000f / worldSize,
                coverageThreshold = 0.28f,
                minimumLengthMetres = 0.45f * 2000f / worldSize,
                maximumLengthMetres = 1.15f * 2000f / worldSize,
                maximumSlopeDegrees = 34f,
            },
            worldSize,
            materialColours,
            materialTextureResolution,
            cancellationToken);
    }

    internal static Task<IslandPreparedData> GenerateAsync(
        int seed,
        MotuNative.Options options,
        MotuNative.ForestOptions forestOptions,
        MotuNative.ReedOptions reedOptions,
        MotuNative.FernOptions fernOptions,
        float worldSize,
        IslandMaterialColours materialColours,
        int materialTextureResolution,
        CancellationToken cancellationToken)
    {
        var descriptor = new IslandDescriptor(
            $"worker-{seed}",
            UnityEngine.Vector2Int.zero,
            0d,
            0d,
            seed,
            worldSize * 0.5f,
            1);
        return GenerateAsync(
            new IslandGenerationRequest(
                descriptor,
                options,
                forestOptions,
                reedOptions,
                fernOptions,
                worldSize,
                materialColours,
                materialTextureResolution),
            cancellationToken);
    }
}
