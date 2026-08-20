using System.Threading;
using System.Threading.Tasks;

internal static class IslandGenerationWorker
{
    internal static Task<IslandPreparedData> GenerateAsync(
        int seed,
        MotuNative.Options options,
        float worldSize,
        float emitterSharpnessDegrees,
        float emitterSpacingMetres,
        CancellationToken cancellationToken)
    {
        return Task.Run(
            () => IslandGenerator.PrepareIsland(
                seed,
                options,
                worldSize,
                emitterSharpnessDegrees,
                emitterSpacingMetres,
                cancellationToken),
            cancellationToken);
    }
}
