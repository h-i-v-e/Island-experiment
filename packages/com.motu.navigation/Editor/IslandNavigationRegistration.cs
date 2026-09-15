using UnityEditor;
using Motu.Navigation;

namespace Motu.Editor
{
    internal static class IslandNavigationRegistration
    {
        [InitializeOnLoadMethod]
        private static void Register() => IslandNavigationInstaller.Register();
    }
}
