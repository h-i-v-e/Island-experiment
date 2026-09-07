using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using UnityEngine;
using Debug = UnityEngine.Debug;
using Motu.Interop;
using Motu.Rendering;
using Motu.Streaming;

using Motu.Islands;
using static Motu.Interop.IslandMeshInterop;
using static Motu.Interop.NativeExportValidation;
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
