using System;

namespace Motu.Interop
{
    internal static class NativeExportValidation
    {
        internal static void ValidateBorrowedArray(
            MotuNative.Vector3Array values,
            string label)
        {
            if (values.length < 0 || (values.length > 0 && values.data == IntPtr.Zero))
            {
                throw new InvalidOperationException(
                    $"The native {label} decoration array is invalid.");
            }
        }
    }
}
