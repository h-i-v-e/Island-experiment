using System;
using UnityEngine;
using Motu.Interop;
using Motu.Settings;
using Motu.World;

namespace Motu.Islands
{
    public sealed class IslandGenerationRequest
    {
        public int RandomSeed => Descriptor.Seed;
        public Vector2Int IslandGridPosition => Descriptor.WorldCell;
        public string IslandId => Descriptor.IslandId;
        public float WorldSizeMetres => IslandWorldManager.IslandSizeMetres;
        public IslandGenerationSettings Generation => Profile?.Generation;
        public IslandRiverSettings Rivers => Profile?.Rivers;
        public IslandForestSettings Forest => Profile?.Forest;
        public IslandReedSettings Reeds => Profile?.Reeds;
        public IslandFernSettings Ferns => Profile?.Ferns;
        public IslandRenderingSettings Rendering => Profile?.Rendering;
        public IslandDebugSettings DebugSettings => Profile?.DebugSettings;

        internal IslandDescriptor Descriptor { get; }
        internal MotuNative.Options Options { get; }
        internal MotuNative.ForestOptions ForestOptions { get; }
        internal MotuNative.ReedOptions ReedOptions { get; }
        internal MotuNative.FernOptions FernOptions { get; }
        public IslandMaterialColours MaterialColours { get; }
        internal int MaterialTextureResolution { get; }
        internal string SnapshotPath { get; }
        internal long SnapshotCacheBudgetBytes { get; }
        internal IslandGenerationProfile Profile { get; }

        public IslandGenerationRequest(
            int randomSeed,
            Vector2Int islandGridPosition,
            IslandGenerationProfile profile,
            IslandMaterialColours materialColours,
            string stableId = null)
            : this(
                IslandDescriptor.Request(
                    randomSeed,
                    islandGridPosition,
                    stableId),
                Require(profile, nameof(profile)),
                materialColours)
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
            IslandMaterialColours materialColours,
            string stableId = null)
            : this(
                IslandDescriptor.Request(
                    randomSeed,
                    islandGridPosition,
                    stableId),
                generation,
                rivers,
                forest,
                reeds,
                ferns,
                rendering,
                null,
                materialColours)
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
            IslandDebugSettings debugSettings,
            IslandMaterialColours materialColours,
            string stableId = null)
            : this(
                IslandDescriptor.Request(
                    randomSeed,
                    islandGridPosition,
                    stableId),
                generation,
                rivers,
                forest,
                reeds,
                ferns,
                rendering,
                debugSettings,
                materialColours)
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
            IslandDebugSettings debugSettings,
            IslandMaterialColours materialColours)
            : this(
                descriptor,
                new IslandGenerationProfile(
                    generation,
                    rivers,
                    forest,
                    reeds,
                    ferns,
                    rendering,
                    debugSettings),
                materialColours)
        {
        }

        private IslandGenerationRequest(
            IslandDescriptor descriptor,
            IslandGenerationProfile profile,
            IslandMaterialColours materialColours)
        {
            Profile = Require(profile, nameof(profile)).Clone();
            Profile.Generation.Seed = descriptor.Seed;

            Descriptor = descriptor;
            Options = Profile.Generation.ToNativeOptions(Profile.Rivers);
            ForestOptions = Profile.Generation.ToNativeForestOptions(Profile.Forest);
            ReedOptions = Profile.Generation.ToNativeReedOptions(Profile.Reeds);
            FernOptions = Profile.Generation.ToNativeFernOptions(Profile.Ferns);
            MaterialColours = materialColours;
            MaterialTextureResolution = Profile.Rendering.MaterialTextureResolution;
            SnapshotCacheBudgetBytes = Profile.Generation.SnapshotCacheBudgetBytes;
            SnapshotPath = Profile.Generation.UseSnapshotCache
                ? IslandSnapshotCache.PathFor(this)
                : null;
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

        private static T Require<T>(T value, string parameterName) where T : class =>
            value ?? throw new ArgumentNullException(parameterName);

    }
}
