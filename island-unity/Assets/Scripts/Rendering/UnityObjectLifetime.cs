using UnityEngine;

namespace Motu.Rendering
{
    internal static class UnityObjectLifetime
    {
        internal static void DestroyUnityObject(Object value)
        {
            if (value == null) return;
            if (Application.isPlaying) Object.Destroy(value);
            else Object.DestroyImmediate(value);
        }
    }
}
