using UnityEngine;

namespace Motu.Settings
{
    [CreateAssetMenu(
        fileName = "WorldEnvironmentConfiguration",
        menuName = "Motu/World Environment Configuration")]
    [UnityEngine.Scripting.APIUpdating.MovedFrom(true, sourceNamespace: "", sourceAssembly: "Assembly-CSharp", sourceClassName: "WorldEnvironmentConfiguration")]
    public sealed class WorldEnvironmentConfiguration : ScriptableObject
    {
        [SerializeField] private WorldEnvironmentSettings environment =
            new WorldEnvironmentSettings();
        [SerializeField] private IslandCloudSettings clouds = new IslandCloudSettings();

        public WorldEnvironmentSettings Environment =>
            environment ??= new WorldEnvironmentSettings();
        public IslandCloudSettings Clouds => clouds ??= new IslandCloudSettings();
    }
}
