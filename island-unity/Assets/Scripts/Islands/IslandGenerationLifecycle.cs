using System;
using System.Diagnostics;
using System.Threading;

internal sealed class IslandGenerationLifecycle : IDisposable
{
    private CancellationTokenSource activeCancellation;
    private Stopwatch timer;

    internal bool IsGenerating => activeCancellation != null;
    internal bool IsDestroyed { get; private set; }
    internal TimeSpan Elapsed => timer?.Elapsed ?? TimeSpan.Zero;

    internal bool TryBegin(
        CancellationToken externalCancellation,
        out CancellationTokenSource cancellation)
    {
        if (IsGenerating || IsDestroyed)
        {
            cancellation = null;
            return false;
        }
        cancellation = externalCancellation.CanBeCanceled
            ? CancellationTokenSource.CreateLinkedTokenSource(externalCancellation)
            : new CancellationTokenSource();
        activeCancellation = cancellation;
        timer = Stopwatch.StartNew();
        return true;
    }

    internal void StopTimer()
    {
        timer?.Stop();
    }

    internal void Cancel()
    {
        activeCancellation?.Cancel();
    }

    internal void MarkDestroyed()
    {
        IsDestroyed = true;
        Cancel();
    }

    internal void End(CancellationTokenSource cancellation)
    {
        if (!ReferenceEquals(activeCancellation, cancellation))
        {
            cancellation?.Dispose();
            return;
        }
        activeCancellation = null;
        timer = null;
        cancellation.Dispose();
    }

    public void Dispose()
    {
        MarkDestroyed();
        activeCancellation?.Dispose();
        activeCancellation = null;
        timer = null;
    }
}
