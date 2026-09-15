using Motu.Islands;
using UnityEngine;

namespace Motu.Navigation
{
    public static class IslandNavigationInstaller
    {
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        public static void Register()
        {
            IslandNavigationIntegration.Install = (runtime, prepared, settings) =>
            {
                var root = new GameObject("Island Navigation");
                root.transform.SetParent(runtime.transform, false);
                var navigation = root.AddComponent<IslandNavigation>();
                runtime.SetNavigation(navigation);
                navigation.StartBuild(prepared.navigationMesh, prepared.forest,
                    prepared.boulderColliders, prepared.caves, settings);
            };
        }
    }
}
