using System.Threading;
using System.Threading.Tasks;

namespace Motu.Islands
{
    internal static class IslandGenerationWorker
    {
        // Native generation and export can each retain large meshes. Queue before using a worker.
        private static readonly SemaphoreSlim PreparationGate = new SemaphoreSlim(1, 1);

        internal static async Task<IslandPreparedData> GenerateAsync(
            IslandGenerationRequest request, CancellationToken cancellationToken)
        {
            if (request == null) throw new System.ArgumentNullException(nameof(request));
            await PreparationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                return await Task.Run(() => IslandPreparationPipeline.PrepareIsland(request, cancellationToken),
                    cancellationToken).ConfigureAwait(false);
            }
            finally { PreparationGate.Release(); }
        }
    }
}
