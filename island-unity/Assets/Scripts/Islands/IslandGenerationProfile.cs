using System;
using UnityEngine;

internal sealed class IslandGenerationProfile
{
    internal IslandGenerationSettings Generation { get; }
    internal IslandRiverSettings Rivers { get; }
    internal IslandForestSettings Forest { get; }
    internal IslandReedSettings Reeds { get; }
    internal IslandFernSettings Ferns { get; }
    internal IslandRenderingSettings Rendering { get; }
    internal IslandDecorationSettings Decorations { get; }
    internal IslandDebugSettings DebugSettings { get; }

    internal IslandGenerationProfile(
        IslandGenerationSettings generation,
        IslandRiverSettings rivers,
        IslandForestSettings forest,
        IslandReedSettings reeds,
        IslandFernSettings ferns,
        IslandRenderingSettings rendering,
        IslandDecorationSettings decorations,
        IslandDebugSettings debugSettings)
    {
        Generation = Clone(Require(generation, nameof(generation)));
        Rivers = Clone(Require(rivers, nameof(rivers)));
        Forest = Clone(Require(forest, nameof(forest)));
        Reeds = Clone(Require(reeds, nameof(reeds)));
        Ferns = Clone(Require(ferns, nameof(ferns)));
        Rendering = Clone(Require(rendering, nameof(rendering)));
        Decorations = Clone(decorations ?? new IslandDecorationSettings());
        DebugSettings = Clone(debugSettings ?? new IslandDebugSettings());
    }

    internal static IslandGenerationProfile FromConfiguration(
        IslandConfiguration configuration)
    {
        if (configuration == null)
        {
            throw new InvalidOperationException(
                "IslandGenerator requires an IslandConfiguration asset.");
        }
        return new IslandGenerationProfile(
            configuration.Generation,
            configuration.Rivers,
            configuration.Forest,
            configuration.Reeds,
            configuration.Ferns,
            configuration.Rendering,
            configuration.Decorations,
            configuration.DebugSettings);
    }

    internal IslandGenerationProfile Clone() => new IslandGenerationProfile(
        Generation,
        Rivers,
        Forest,
        Reeds,
        Ferns,
        Rendering,
        Decorations,
        DebugSettings);

    private static T Require<T>(T value, string parameterName) where T : class =>
        value ?? throw new ArgumentNullException(parameterName);

    private static T Clone<T>(T value) where T : class =>
        JsonUtility.FromJson<T>(JsonUtility.ToJson(value));
}
