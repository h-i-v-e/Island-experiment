using System;
using Motu.Settings;

namespace Motu.Islands
{
    // Installed by the optional navigation assembly before scene loading. Main-thread registration only.
    internal static class IslandNavigationIntegration
    {
        internal static Action<IslandRuntime, IslandPreparedData, IslandNavigationSettings> Install;
        internal static bool IsAvailable => Install != null;
    }
}
