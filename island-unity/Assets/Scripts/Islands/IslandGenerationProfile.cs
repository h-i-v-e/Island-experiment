using System;
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
        public IslandCaveSettings Caves { get; }
        public IslandRenderingSettings Rendering { get; }
        public IslandDebugSettings DebugSettings { get; }

        public IslandGenerationProfile(
            IslandGenerationSettings generation,
            IslandRiverSettings rivers,
            IslandForestSettings forest,
            IslandReedSettings reeds,
            IslandFernSettings ferns,
            IslandRenderingSettings rendering,
            IslandDebugSettings debugSettings,
            IslandCaveSettings caves = null)
        {
            Generation = Require(generation, nameof(generation)).Copy();
            Rivers = Require(rivers, nameof(rivers)).Copy();
            Forest = Require(forest, nameof(forest)).Copy();
            Reeds = Require(reeds, nameof(reeds)).Copy();
            Ferns = Require(ferns, nameof(ferns)).Copy();
            Caves = (caves ?? new IslandCaveSettings()).Copy();
            Rendering = Require(rendering, nameof(rendering)).Copy();
            DebugSettings = (debugSettings ?? new IslandDebugSettings()).Copy();
        }

        public IslandGenerationProfile Clone() => new IslandGenerationProfile(
            Generation,
            Rivers,
            Forest,
            Reeds,
            Ferns,
            Rendering,
            DebugSettings, Caves);

        private static T Require<T>(T value, string parameterName) where T : class =>
            value ?? throw new ArgumentNullException(parameterName);

    }
}
