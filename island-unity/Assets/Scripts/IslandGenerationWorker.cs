using System.Threading;
using System.Threading.Tasks;

internal static class IslandGenerationWorker
{
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
            worldSize,
            materialColours,
            materialTextureResolution,
            cancellationToken);
    }

    internal static Task<IslandPreparedData> GenerateAsync(
        int seed,
        MotuNative.Options options,
        MotuNative.ForestOptions forestOptions,
        float worldSize,
        IslandMaterialColours materialColours,
        int materialTextureResolution,
        CancellationToken cancellationToken)
    {
        return Task.Run(
            () => IslandGenerator.PrepareIsland(
                seed,
                options,
                forestOptions,
                worldSize,
                materialColours,
                materialTextureResolution,
                cancellationToken),
            cancellationToken);
    }
}
