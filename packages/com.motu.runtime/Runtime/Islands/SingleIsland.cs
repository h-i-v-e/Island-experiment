using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace Motu.Islands
{
    /// <summary>Host-owned single-island setup. Does not create a camera, ocean, or environment.</summary>
    [DisallowMultipleComponent]
    [RequireComponent(typeof(IslandGenerator))]
    [AddComponentMenu("Motu/Single Island")]
    public sealed class SingleIsland : IslandGenerationRequestFactoryBase
    {
        [SerializeField] private bool generateOnStart = true;
        [SerializeField] private Transform streamingTarget;
        [SerializeField, Min(0.1f)] private float installationBudgetMilliseconds = 4f;
        public IslandGenerator Generator => GetComponent<IslandGenerator>();
        public Transform StreamingTarget { get => streamingTarget; set => streamingTarget = value; }
        public bool GenerateOnStart { get => generateOnStart; set => generateOnStart = value; }

        private async void Start()
        {
            if (generateOnStart) await GenerateAsync();
        }

        public override bool HasIsland(Vector2Int cell) => cell == Vector2Int.zero;
        public override IslandGenerationRequest CreateIslandGenerationRequest(Vector2Int cell) =>
            HasIsland(cell) ? CreateRequest(GenerationSettings.Seed, cell, CreateProfile()) : null;

        /// <summary>Call on Unity's main thread. A busy generator rejects overlapping requests.</summary>
        public Task<bool> GenerateAsync(CancellationToken cancellation = default)
        {
            Generator.SetStreamingTarget(streamingTarget);
            return Generator.GenerateAsync(CreateIslandGenerationRequest(Vector2Int.zero), cancellation,
                installationBudgetMilliseconds);
        }
        public void Clear() => Generator.Clear();
    }
}
