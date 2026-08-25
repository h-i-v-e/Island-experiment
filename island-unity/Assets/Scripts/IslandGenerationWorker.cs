using System.Threading;
using System.Threading.Tasks;

internal static class IslandGenerationWorker
{
    internal static Task<IslandPreparedData> GenerateAsync(
        int seed,
        IslandGenerationMethod generationMethod,
        MotuNative.Options options,
        float worldSize,
        float emitterSharpnessDegrees,
        float emitterSpacingMetres,
        CancellationToken cancellationToken)
    {
        return GenerateAsync(
            seed,
            generationMethod,
            options,
            new MotuNative.ForestOptions
            {
                patchSizeMetres = 200f * 2000f / worldSize,
                noiseThreshold = 0.62f,
                noiseOctaves = 4,
                snowlineMetres = 100f * 2000f / worldSize,
                prototypeCount = 8,
                minimumScale = 0.85f,
                maximumScale = 1.15f,
            },
            worldSize,
            emitterSharpnessDegrees,
            emitterSpacingMetres,
            cancellationToken);
    }

    internal static Task<IslandPreparedData> GenerateAsync(
        int seed,
        IslandGenerationMethod generationMethod,
        MotuNative.Options options,
        MotuNative.ForestOptions forestOptions,
        float worldSize,
        float emitterSharpnessDegrees,
        float emitterSpacingMetres,
        CancellationToken cancellationToken)
    {
        return Task.Run(
            () => IslandGenerator.PrepareIsland(
                seed,
                generationMethod,
                options,
                forestOptions,
                worldSize,
                emitterSharpnessDegrees,
                emitterSpacingMetres,
                cancellationToken),
            cancellationToken);
    }
}
