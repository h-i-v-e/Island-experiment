using System;
using UnityEngine;
using Motu.Settings;

namespace Motu.Islands
{
    public sealed class IslandGenerationProfile
    {
        public IslandGenerationSettings Generation { get; }
        public IslandRiverSettings Rivers { get; }
        public IslandForestSettings Forest { get; }
        public IslandReedSettings Reeds { get; }
        public IslandFernSettings Ferns { get; }
        public IslandRenderingSettings Rendering { get; }
        public IslandDebugSettings DebugSettings { get; }

        public IslandGenerationProfile(
            IslandGenerationSettings generation,
            IslandRiverSettings rivers,
            IslandForestSettings forest,
            IslandReedSettings reeds,
            IslandFernSettings ferns,
            IslandRenderingSettings rendering,
            IslandDebugSettings debugSettings)
        {
            Generation = Clone(Require(generation, nameof(generation)));
            Rivers = Clone(Require(rivers, nameof(rivers)));
            Forest = Clone(Require(forest, nameof(forest)));
            Reeds = Clone(Require(reeds, nameof(reeds)));
            Ferns = Clone(Require(ferns, nameof(ferns)));
            Rendering = Clone(Require(rendering, nameof(rendering)));
            DebugSettings = Clone(debugSettings ?? new IslandDebugSettings());
        }

        public IslandGenerationProfile Clone() => new IslandGenerationProfile(
            Generation,
            Rivers,
            Forest,
            Reeds,
            Ferns,
            Rendering,
            DebugSettings);

        private static T Require<T>(T value, string parameterName) where T : class =>
            value ?? throw new ArgumentNullException(parameterName);

        private static T Clone<T>(T value) where T : class =>
            JsonUtility.FromJson<T>(JsonUtility.ToJson(value));
    }
}
