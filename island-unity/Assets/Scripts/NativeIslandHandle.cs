using System;

internal sealed class NativeIslandHandle : IDisposable
{
    private IntPtr value;

    internal NativeIslandHandle(IntPtr handle)
    {
        if (handle == IntPtr.Zero)
        {
            throw new ArgumentException("A native island handle cannot be null.", nameof(handle));
        }
        value = handle;
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
        MotuNative.ReleaseMotu(value);
        value = IntPtr.Zero;
    }
}
