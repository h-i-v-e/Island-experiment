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

}
