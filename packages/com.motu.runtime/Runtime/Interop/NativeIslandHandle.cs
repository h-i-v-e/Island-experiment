using System;
using System.Threading;

namespace Motu.Interop
{
    internal sealed class NativeIslandHandle : IDisposable
    {
        private static int activeCount;
        private readonly object sync = new object();
        private IntPtr value;
        private int references = 1;
        private bool disposed;

        internal static int ActiveCount => Volatile.Read(ref activeCount);

        internal NativeIslandHandle(IntPtr handle)
        {
            if (handle == IntPtr.Zero) throw new ArgumentException("A native island handle cannot be null.", nameof(handle));
            value = handle;
            Interlocked.Increment(ref activeCount);
        }

        internal bool IsValid { get { lock (sync) return !disposed; } }
        internal IntPtr Value
        {
            get
            {
                lock (sync)
                {
                    if (disposed) throw new ObjectDisposedException(nameof(NativeIslandHandle));
                    return value;
                }
            }
        }

        // A worker must hold a lease until all native reads and export copies end.
        // Runtime disposal never waits for that worker on Unity's main thread.
        internal Lease Acquire()
        {
            lock (sync)
            {
                if (disposed) throw new ObjectDisposedException(nameof(NativeIslandHandle));
                references++;
                return new Lease(this, value);
            }
        }

        public void Dispose()
        {
            lock (sync)
            {
                if (disposed) return;
                disposed = true;
            }
            ReleaseReference();
        }

        private void ReleaseReference()
        {
            IntPtr released;
            lock (sync)
            {
                if (--references != 0) return;
                released = value;
                value = IntPtr.Zero;
            }
            try { MotuNative.ReleaseMotu(released); }
            finally { Interlocked.Decrement(ref activeCount); }
        }

        internal sealed class Lease : IDisposable
        {
            private NativeIslandHandle owner;
            internal IntPtr Value { get; }
            internal Lease(NativeIslandHandle owner, IntPtr value)
            {
                this.owner = owner;
                Value = value;
            }
            public void Dispose() => Interlocked.Exchange(ref owner, null)?.ReleaseReference();
        }
    }
}
