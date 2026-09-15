using Motu.Islands;
using UnityEngine;

namespace Motu.Samples
{
    /// <summary>Place on an empty object in your own scene and assign a viewer.</summary>
    public sealed class SingleIslandExample : MonoBehaviour
    {
        [SerializeField] private Transform streamingTarget;
        [SerializeField] private int seed = 17;
        private SingleIsland island;

        private void Start()
        {
            var root = new GameObject("Generated island");
            root.transform.SetParent(transform, false);
            island = root.AddComponent<SingleIsland>();
            island.StreamingTarget = streamingTarget;
            island.GenerationSettings.Seed = seed;
            // SingleIsland starts generation on its next Start callback.
        }

        private void OnDestroy()
        {
            if (island != null) island.Clear();
        }
    }
}
