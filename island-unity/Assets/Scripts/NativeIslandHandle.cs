using System;
using System.Threading;

internal sealed class NativeIslandHandle : IDisposable
{
    private static int activeCount;
    private IntPtr value;

    internal static int ActiveCount => Volatile.Read(ref activeCount);

    internal NativeIslandHandle(IntPtr handle)
    {
        if (handle == IntPtr.Zero)
        {
            throw new ArgumentException("A native island handle cannot be null.", nameof(handle));
        }
        value = handle;
        Interlocked.Increment(ref activeCount);
    }

    internal bool IsValid => value != IntPtr.Zero;

    internal IntPtr Value
    {
        get
        {
            if (value == IntPtr.Zero)
            {
                throw new ObjectDisposedException(nameof(NativeIslandHandle));
            }
            return value;
        }
    }

    public void Dispose()
    {
        if (value == IntPtr.Zero)
        {
            return;
        }
        var handle = value;
        value = IntPtr.Zero;
        try
        {
            MotuNative.ReleaseMotu(handle);
        }
        finally
        {
            Interlocked.Decrement(ref activeCount);
        }
    }
}
