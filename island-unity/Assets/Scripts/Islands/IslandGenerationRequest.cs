using System;
using UnityEngine;

public sealed class IslandGenerationRequest
{
    public int RandomSeed => Descriptor.Seed;
    public Vector2Int IslandGridPosition => Descriptor.WorldCell;
    public string IslandId => Descriptor.IslandId;
    public float WorldSizeMetres { get; }
    public IslandGenerationSettings Generation => Profile?.Generation;
    public IslandRiverSettings Rivers => Profile?.Rivers;
    public IslandForestSettings Forest => Profile?.Forest;
    public IslandReedSettings Reeds => Profile?.Reeds;
    public IslandFernSettings Ferns => Profile?.Ferns;
    public IslandRenderingSettings Rendering => Profile?.Rendering;
    public IslandDecorationSettings Decorations => Profile?.Decorations;
    public IslandDebugSettings DebugSettings => Profile?.DebugSettings;

    internal IslandDescriptor Descriptor { get; }
    internal MotuNative.Options Options { get; }
    internal MotuNative.ForestOptions ForestOptions { get; }
    internal MotuNative.ReedOptions ReedOptions { get; }
    internal MotuNative.FernOptions FernOptions { get; }
    internal IslandMaterialColours MaterialColours { get; }
    internal int MaterialTextureResolution { get; }
    internal string SnapshotPath { get; }
    internal long SnapshotCacheBudgetBytes { get; }
    internal IslandGenerationProfile Profile { get; }

    public IslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition,
        IslandConfiguration configuration,
        string stableId = null)
        : this(
            randomSeed,
            islandGridPosition,
            configuration,
            null,
            stableId)
    {
    }

    public IslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition,
        IslandConfiguration configuration,
        IslandParameterVariationSettings parameterVariation,
        string stableId = null)
        : this(
            IslandDescriptor.Request(
                randomSeed,
                islandGridPosition,
                IslandWorldManager.IslandCellSizeMetres,
                Require(configuration).Generation.WorldSizeMetres,
                stableId),
            CreateProfile(configuration, randomSeed, parameterVariation))
    {
    }

    public IslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition,
        IslandGenerationSettings generation,
        IslandRiverSettings rivers,
        IslandForestSettings forest,
        IslandReedSettings reeds,
        IslandFernSettings ferns,
        IslandRenderingSettings rendering,
        string stableId = null)
        : this(
            IslandDescriptor.Request(
                randomSeed,
                islandGridPosition,
                IslandWorldManager.IslandCellSizeMetres,
                Require(generation, nameof(generation)).WorldSizeMetres,
                stableId),
            generation,
            rivers,
            forest,
            reeds,
            ferns,
            rendering,
            null,
            null)
    {
    }

    public IslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition,
        IslandGenerationSettings generation,
        IslandRiverSettings rivers,
        IslandForestSettings forest,
        IslandReedSettings reeds,
        IslandFernSettings ferns,
        IslandRenderingSettings rendering,
        IslandDecorationSettings decorations,
        IslandDebugSettings debugSettings,
        string stableId = null)
        : this(
            IslandDescriptor.Request(
                randomSeed,
                islandGridPosition,
                IslandWorldManager.IslandCellSizeMetres,
                Require(generation, nameof(generation)).WorldSizeMetres,
                stableId),
            generation,
            rivers,
            forest,
            reeds,
            ferns,
            rendering,
            decorations,
            debugSettings)
    {
    }

    internal IslandGenerationRequest(
        IslandDescriptor descriptor,
        IslandGenerationSettings generation,
        IslandRiverSettings rivers,
        IslandForestSettings forest,
        IslandReedSettings reeds,
        IslandFernSettings ferns,
        IslandRenderingSettings rendering,
        IslandDecorationSettings decorations,
        IslandDebugSettings debugSettings)
        : this(
            descriptor,
            new IslandGenerationProfile(
                generation,
                rivers,
                forest,
                reeds,
                ferns,
                rendering,
                decorations,
                debugSettings))
    {
    }

    private IslandGenerationRequest(
        IslandDescriptor descriptor,
        IslandGenerationProfile profile)
    {
        Profile = profile.Clone();
        Profile.Generation.Seed = descriptor.Seed;

        Descriptor = descriptor;
        WorldSizeMetres = Profile.Generation.WorldSizeMetres;
        Options = Profile.Generation.ToNativeOptions(Profile.Rivers);
        ForestOptions = Profile.Generation.ToNativeForestOptions(Profile.Forest);
        ReedOptions = Profile.Generation.ToNativeReedOptions(Profile.Reeds);
        FernOptions = Profile.Generation.ToNativeFernOptions(Profile.Ferns);
        MaterialColours = Profile.Rendering.SelectMaterialColours(descriptor.Seed);
        MaterialTextureResolution = Profile.Rendering.MaterialTextureResolution;
        SnapshotCacheBudgetBytes = Profile.Generation.SnapshotCacheBudgetBytes;
        SnapshotPath = Profile.Generation.UseSnapshotCache
            ? IslandSnapshotCache.PathFor(this)
            : null;
    }

    internal IslandGenerationRequest(
        IslandDescriptor descriptor,
        MotuNative.Options options,
        MotuNative.ForestOptions forestOptions,
        MotuNative.ReedOptions reedOptions,
        MotuNative.FernOptions fernOptions,
        float worldSizeMetres,
        IslandMaterialColours materialColours,
        int materialTextureResolution,
        bool useSnapshotCache = true,
        long snapshotCacheBudgetBytes = 8L * 1024L * 1024L * 1024L)
    {
        if (string.IsNullOrWhiteSpace(descriptor.IslandId))
        {
            throw new ArgumentException(
                "An island generation request requires a valid descriptor.",
                nameof(descriptor));
        }
        if (float.IsNaN(worldSizeMetres)
            || float.IsInfinity(worldSizeMetres)
            || worldSizeMetres <= 0f)
        {
            throw new ArgumentOutOfRangeException(nameof(worldSizeMetres));
        }
        if (materialTextureResolution <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(materialTextureResolution));
        }
        Descriptor = descriptor;
        Options = options;
        ForestOptions = forestOptions;
        ReedOptions = reedOptions;
        FernOptions = fernOptions;
        WorldSizeMetres = worldSizeMetres;
        MaterialColours = materialColours;
        MaterialTextureResolution = materialTextureResolution;
        SnapshotCacheBudgetBytes = Math.Max(snapshotCacheBudgetBytes, 0L);
        SnapshotPath = useSnapshotCache ? IslandSnapshotCache.PathFor(this) : null;
    }

    internal void ApplyProfileTo(IslandGenerator generator)
    {
        if (generator == null) throw new ArgumentNullException(nameof(generator));
        if (Profile == null)
        {
            return;
        }
        generator.ApplyRequestProfile(Profile);
    }

    private static IslandConfiguration Require(IslandConfiguration value) =>
        value != null ? value : throw new ArgumentNullException(nameof(value));

    private static IslandGenerationProfile CreateProfile(
        IslandConfiguration configuration,
        int randomSeed,
        IslandParameterVariationSettings parameterVariation)
    {
        var profile = IslandGenerationProfile.FromConfiguration(Require(configuration));
        profile.Generation.Seed = randomSeed;
        profile.Generation.ApplyDeterministicVariation(
            randomSeed,
            parameterVariation);
        return profile;
    }

    private static T Require<T>(T value, string parameterName) where T : class =>
        value ?? throw new ArgumentNullException(parameterName);

}
