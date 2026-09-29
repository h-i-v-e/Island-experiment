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
                if (settings == null || !settings.Enabled) return;
                var root = new GameObject("Island Navigation");
                root.transform.SetParent(runtime.transform, false);
                var navigation = root.AddComponent<IslandNavigation>();
                runtime.SetNavigation(navigation);
                if (prepared.navigationMesh != null)
                {
                    navigation.StartBuild(prepared.navigationMesh, prepared.forest,
                        prepared.boulderColliders, prepared.caves, settings);
                }
                else
                {
                    navigation.ArmApproachBuild(runtime.NativeHandle, runtime.WorldSizeMetres,
                        prepared.forest, prepared.boulderColliders, prepared.caves, settings);
                }
            };
        }
    }
}
