using System;
using System.Collections.Generic;
using System.Reflection;
using Motu.Islands;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Tests
{
    public sealed class IslandMaterialLifetimeTests
    {
        private const BindingFlags PrivateInstance = BindingFlags.Instance | BindingFlags.NonPublic;
        private static readonly string[] MaterialFields = {
            "terrainMaterial", "terrainLod1Material", "terrainLod2Material", "grassMaterial",
            "rockMaterial", "fallenLogMaterial", "riverMaterial", "reedMaterial", "fernMaterial"
        };

        private sealed class Fixture : IDisposable
        {
            private readonly GameObject root = new GameObject("Island material lifetime");
            private readonly GameObject environmentRoot = new GameObject("Bound island lighting");
            private readonly List<Material> materials = new List<Material>();
            internal readonly IslandGenerator Generator;
            internal readonly WorldEnvironmentController Environment;

            internal Fixture()
            {
                root.SetActive(false);
                environmentRoot.SetActive(false);
                Generator = root.AddComponent<IslandGenerator>();
                Environment = environmentRoot.AddComponent<WorldEnvironmentController>();
                Generator.ApplyRequestProfile(new IslandGenerationProfile(
                    new IslandGenerationSettings(), new IslandRiverSettings(), new IslandForestSettings(),
                    new IslandReedSettings(), new IslandFernSettings(), new IslandRenderingSettings(),
                    new IslandDebugSettings()));
                Generator.ConfigureEnvironment(Environment);
                SetField(Environment, "currentSkyExposure", 1.7f);
                SetField(Environment, "currentNightStrength", .35f);
            }

            internal Material AssignMaterial(string field)
            {
                var shader = Shader.Find("Motu/River Water");
                Assert.IsNotNull(shader);
                var material = new Material(shader);
                materials.Add(material);
                SetField(Generator, field, material);
                return material;
            }

            internal void Update() => Invoke("Update");

            internal void Invoke(string method) => ((Action)Delegate.CreateDelegate(typeof(Action), Generator,
                typeof(IslandGenerator).GetMethod(method, PrivateInstance)))();

            public void Dispose()
            {
                foreach (var field in MaterialFields) SetField(Generator, field, null);
                foreach (var material in materials) if (material != null) Object.DestroyImmediate(material);
                Object.DestroyImmediate(root);
                Object.DestroyImmediate(environmentRoot);
            }
        }

        [Test]
        public void SharedLightingToleratesUnassignedAndDestroyedRiverMaterials()
        {
            using var fixture = new Fixture();
            Assert.DoesNotThrow(() => fixture.Generator.SyncSharedWorldLighting(fixture.Environment));
            var material = fixture.AssignMaterial("riverMaterial");
            Object.DestroyImmediate(material);
            Assert.IsTrue(material == null);
            Assert.IsFalse(ReferenceEquals(material, null), "The regression requires Unity's destroyed-object wrapper.");
            Assert.DoesNotThrow(() => fixture.Generator.SyncSharedWorldLighting(fixture.Environment));
        }

        [Test]
        public void UpdateAndClearTolerateDestroyedRuntimeMaterials()
        {
            using var fixture = new Fixture();
            foreach (var field in MaterialFields) Object.DestroyImmediate(fixture.AssignMaterial(field));
            Assert.DoesNotThrow(fixture.Update);
            Assert.DoesNotThrow(fixture.Generator.Clear);
            Assert.DoesNotThrow(fixture.Update);
        }

        [Test]
        public void SharedLightingStillUpdatesTheLiveRiverMaterial()
        {
            using var fixture = new Fixture();
            var material = fixture.AssignMaterial("riverMaterial");
            fixture.Generator.SyncSharedWorldLighting(fixture.Environment);
            Assert.That(material.GetFloat("_WaterSkyExposure"), Is.EqualTo(1.7f).Within(.0001f));
            Assert.That((float)typeof(IslandGenerator).GetField("currentNightStrength", PrivateInstance)
                .GetValue(fixture.Generator), Is.EqualTo(.35f).Within(.0001f));
        }

        [Test]
        public void SnowlineUpdateToleratesDestroyedLodMaterials()
        {
            using var fixture = new Fixture();
            Object.DestroyImmediate(fixture.AssignMaterial("terrainLod1Material"));
            Object.DestroyImmediate(fixture.AssignMaterial("terrainLod2Material"));
            Assert.DoesNotThrow(() => fixture.Invoke("ApplySnowlineSettings"));
        }

        private static void SetField(object target, string name, object value) =>
            target.GetType().GetField(name, PrivateInstance).SetValue(target, value);
    }
}
