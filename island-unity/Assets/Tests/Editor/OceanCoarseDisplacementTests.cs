using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    public sealed class OceanCoarseDisplacementTests
    {
        private static OceanWaveRuntimeSettings ExtendedSettings(float end = 512f)
        {
            var profile = ScriptableObject.CreateInstance<OceanWaveProfile>();
            try
            {
                var serialized = new SerializedObject(profile);
                serialized.FindProperty("displacementFadeStartMetres").floatValue = 256f;
                serialized.FindProperty("displacementFadeEndMetres").floatValue = end;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                return profile.ToRuntimeSettings();
            }
            finally { Object.DestroyImmediate(profile); }
        }

        [Test]
        public void DisplacementExtendsBeyondFineGridWithoutIncreasingItsRadius()
        {
            var settings = ExtendedSettings();
            Assert.That(settings.FineRadiusMetres, Is.EqualTo(256f));
            Assert.That(settings.DisplacementFadeStartMetres, Is.EqualTo(256f));
            Assert.That(settings.DisplacementFadeEndMetres, Is.EqualTo(512f));
            Assert.That(settings.MaskCoverageMetres, Is.GreaterThanOrEqualTo(1024f));
            Assert.That(ExtendedSettings(10000f).DisplacementFadeEndMetres, Is.EqualTo(512f));
        }

        [Test]
        public void CoarseBandPreservesEveryFineVertexAndStaysWithinMeshBudget()
        {
            var settings = ExtendedSettings();
            var coordinates = OceanClipmapMeshBuilder.BuildAxisCoordinates(10000f, settings);
            for (int i = -256; i <= 256; i++) Assert.That(coordinates, Does.Contain((float)i));
            int coarseRows = 0;
            for (int i = 1; i < coordinates.Count; i++)
            {
                float spacing = coordinates[i] - coordinates[i - 1];
                Assert.That(spacing, Is.GreaterThan(0f));
                if (coordinates[i - 1] < 256f || coordinates[i] > 512f) continue;
                Assert.That(spacing, Is.InRange(2f, settings.CoarseVertexSpacingMetres));
                coarseRows++;
            }
            Assert.That(coarseRows, Is.InRange(1, 128));
            Assert.That(coordinates[0], Is.EqualTo(-10000f));
            Assert.That(coordinates[coordinates.Count - 1], Is.EqualTo(10000f));
            Assert.That((long)coordinates.Count * coordinates.Count,
                Is.LessThanOrEqualTo(OceanClipmapMeshBuilder.MaximumVertexCount));
            Assert.That((long)(coordinates.Count - 1) * (coordinates.Count - 1) * 2,
                Is.LessThanOrEqualTo(OceanClipmapMeshBuilder.MaximumTriangleCount));
        }

        [Test]
        public void WeatherChangesPreserveCoarseGeometryCoverage()
        {
            var settings = ExtendedSettings();
            var weather = settings.Weather;
            weather.AmplitudeVariation = .5f;
            var changed = settings.WithWeather(weather);
            Assert.That(changed.FineRadiusMetres, Is.EqualTo(settings.FineRadiusMetres));
            Assert.That(changed.DisplacementFadeEndMetres, Is.EqualTo(512f));
            Assert.That(changed.CoarseVertexSpacingMetres, Is.EqualTo(8f));
        }

        [Test]
        public void ProfilesWithoutExtendedDisplacementRetainTheirExistingGrid()
        {
            var settings = OceanWaveRuntimeSettings.Default;
            var coordinates = OceanClipmapMeshBuilder.BuildAxisCoordinates(10000f, settings);
            Assert.That(settings.DisplacementFadeEndMetres, Is.EqualTo(256f));
            Assert.That((long)coordinates.Count * coordinates.Count, Is.EqualTo(436921));
        }
    }
}
