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
            Assert.That(ExtendedSettings(10000f).DisplacementFadeEndMetres, Is.EqualTo(1024f));
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

        [TestCase(10000f)]
        [TestCase(12000f)]
        [TestCase(24000f)]
        public void DistantBandDoublesReachAndSpacingWhilePreservingInnerGrid(float halfExtent)
        {
            var settings = ExtendedSettings(1024f);
            var coordinates = OceanClipmapMeshBuilder.BuildAxisCoordinates(halfExtent, settings);
            var previous = OceanClipmapMeshBuilder.BuildAxisCoordinates(halfExtent, ExtendedSettings());
            CollectionAssert.AreEqual(previous.FindAll(x => Mathf.Abs(x) <= 512f),
                coordinates.FindAll(x => Mathf.Abs(x) <= 512f));
            int start = coordinates.IndexOf(512f), end = coordinates.IndexOf(1024f);
            Assert.That(end - start, Is.EqualTo(32));
            for (int i = start + 1; i <= end; i++)
                Assert.That(coordinates[i] - coordinates[i - 1], Is.EqualTo(16f));
            Assert.That(settings.MaskCoverageMetres, Is.GreaterThanOrEqualTo(2048f));
            Assert.That(coordinates[0], Is.EqualTo(-halfExtent));
            Assert.That(coordinates[coordinates.Count - 1], Is.EqualTo(halfExtent));
            var mesh = OceanClipmapMeshBuilder.Build(halfExtent * 2f, settings, false);
            try
            {
                Assert.That(mesh.vertexCount, Is.LessThanOrEqualTo(OceanClipmapMeshBuilder.MaximumVertexCount));
                Assert.That(mesh.GetIndexCount(0) / 3, Is.LessThanOrEqualTo(OceanClipmapMeshBuilder.MaximumTriangleCount));
                Assert.That(mesh.bounds.extents.x, Is.GreaterThan(halfExtent));
            }
            finally { Object.DestroyImmediate(mesh); }
            var weather = settings.Weather;
            weather.AmplitudeVariation = .5f;
            Assert.That(settings.WithWeather(weather).DisplacementFadeEndMetres, Is.EqualTo(1024f));
        }

        [Test]
        public void PartialDistantBandEndsExactlyAtRequestedFadeDistance()
        {
            var settings = ExtendedSettings(601f);
            var coordinates = OceanClipmapMeshBuilder.BuildAxisCoordinates(12000f, settings);
            Assert.That(coordinates, Does.Contain(601f));
            Assert.That(coordinates, Does.Contain(-601f));
            for (int i = 1; i < coordinates.Count; i++)
                Assert.That(coordinates[i], Is.GreaterThan(coordinates[i - 1]));
        }

        [Test]
        public void DistantGeometryFiltersShortWavesAndFadesContinuouslyToFlatHorizon()
        {
            var root = new GameObject("Distant ocean geometry probe");
            var material = new Material(Shader.Find("Hidden/Motu/Ocean Wave Transition Probe"));
            var target = new RenderTexture(128, 1, 0, RenderTextureFormat.ARGBFloat);
            var output = new Texture2D(128, 1, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                root.AddComponent<OceanSurfaceController>().Install(material, 24000f, true, ExtendedSettings(1024f));
                Assert.That(material.GetVector("_OceanGeometryLod").w, Is.EqualTo(16f));
                material.SetFloat("_ProbeGeometry", 1f);
                material.SetTexture("_MotuWindNoise", Texture2D.grayTexture);
                material.SetVector("_MotuWeatherWind", new Vector4(1, 0, 9, 1));
                material.SetVector("_ProbeWorldRect", new Vector4(0, 0, 256, 0));
                // Isolate a broad swell and disable transitions/coastal waves.
                material.SetVector("_OceanWave0", new Vector4(1, 0, 64, 1));
                material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 64, 0));
                material.SetVector("_OceanWave1", Vector4.zero);
                material.SetVector("_OceanWave2", Vector4.zero);
                material.SetVector("_OceanWave3", Vector4.zero);
                material.SetFloat("_OceanWaveTransition", 0f);
                material.SetFloat("_OnshoreWaveEnabled", 0f);
                Color[] Read(float radius)
                {
                    material.SetFloat("_ProbeRadius", radius);
                    Graphics.Blit(Texture2D.blackTexture, target, material);
                    RenderTexture.active = target;
                    output.ReadPixels(new Rect(0, 0, 128, 1), 0, 0);
                    output.Apply();
                    return output.GetPixels();
                }
                var inner = Read(512f);
                var boundary = Read(512.01f);
                var distant = Read(768f);
                var edge = Read(1024f);
                float distantEnergy = 0f;
                for (int i = 0; i < distant.Length; i++)
                {
                    Assert.That(inner[i].g, Is.EqualTo(8f).Within(.001f));
                    Assert.That(boundary[i].r, Is.EqualTo(inner[i].r).Within(.001f));
                    Assert.That(distant[i].g, Is.EqualTo(16f).Within(.001f));
                    Assert.That(edge[i].r, Is.Zero.Within(.00001f));
                    distantEnergy += distant[i].r * distant[i].r;
                }
                Assert.That(distantEnergy, Is.GreaterThan(.01f), "Broad swell must survive beyond the old 512 m limit.");
                material.SetVector("_OceanWave0", new Vector4(1, 0, 16, 1));
                material.SetVector("_OceanWaveFrom0", new Vector4(1, 0, 16, 0));
                foreach (var sample in Read(768f))
                    Assert.That(sample.r, Is.Zero.Within(.00001f), "The distant grid must remove unresolved short waves.");
            }
            finally
            {
                RenderTexture.active = previous;
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(target);
                Object.DestroyImmediate(output);
            }
        }
    }
}
