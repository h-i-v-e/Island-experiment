using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;

internal sealed class UnityFrameBudget
{
    private readonly double milliseconds;
    private readonly Stopwatch stopwatch = Stopwatch.StartNew();

    internal UnityFrameBudget(float millisecondsPerFrame)
    {
        milliseconds = millisecondsPerFrame > 0f
            ? millisecondsPerFrame
            : 4.0;
    }

    internal async Task YieldIfExceededAsync(
        CancellationToken cancellationToken,
        bool force = false)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (!force && stopwatch.Elapsed.TotalMilliseconds < milliseconds)
        {
            return;
        }

        await Task.Yield();
        cancellationToken.ThrowIfCancellationRequested();
        stopwatch.Restart();
    }
}
