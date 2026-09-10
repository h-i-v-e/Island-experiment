using System;
using System.Collections.Generic;
using Motu.Rendering;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class ShipWaterlineTextureTests
    {
        private static ShipWaterlineTextureBuilder.Settings FlatSettings => new ShipWaterlineTextureBuilder.Settings
        { resolution = 256, clearance = 0, blend = .15f, gapClosure = 0, largestRegionOnly = false, bowWave = false };

        private static void Loop(List<ShipWaterlineTextureBuilder.Segment> segments, params Vector2[] points)
        {
            for (var i = 0; i < points.Length; i++) segments.Add(new ShipWaterlineTextureBuilder.Segment(points[i], points[(i+1)%points.Length]));
        }

        private static float Pixel(ShipWaterlineTextureBuilder.Result result, Vector2 point, bool bow = false)
        {
            var uv = (point-result.TextureBounds.min) / result.TextureBounds.size;
            var colour = result.Texture.GetPixelBilinear(uv.x, uv.y);
            return bow ? colour.a : colour.r;
        }

        [Test]
        public void ConcaveSectionsStayConcaveAndSeparateHullsKeepTheGap()
        {
            var segments = new List<ShipWaterlineTextureBuilder.Segment>();
            Loop(segments, new Vector2(-5,-5), new Vector2(5,-5), new Vector2(5,5),
                new Vector2(2,5), new Vector2(2,-2), new Vector2(-2,-2), new Vector2(-2,5), new Vector2(-5,5));
            var result = ShipWaterlineTextureBuilder.Build(segments, FlatSettings);
            try
            {
                Assert.That(Pixel(result, new Vector2(0,-4)), Is.LessThan(.02));
                Assert.That(Pixel(result, new Vector2(0,2)), Is.EqualTo(128f/255).Within(.005), "No convex hull across the open notch.");
                Assert.That(Pixel(result, new Vector2(4,2)), Is.LessThan(.02));
            }
            finally { Object.DestroyImmediate(result.Texture); }
            segments.Clear();
            Loop(segments, new Vector2(-5,-5), new Vector2(-3,-5), new Vector2(-3,5), new Vector2(-5,5));
            Loop(segments, new Vector2(3,-5), new Vector2(5,-5), new Vector2(5,5), new Vector2(3,5));
            result = ShipWaterlineTextureBuilder.Build(segments, FlatSettings);
            try
            {
                Assert.That(Pixel(result, new Vector2(-4,0)), Is.LessThan(.02));
                Assert.That(Pixel(result, new Vector2(4,0)), Is.LessThan(.02));
                Assert.That(Pixel(result, Vector2.zero), Is.EqualTo(128f/255).Within(.005));
            }
            finally { Object.DestroyImmediate(result.Texture); }
        }

        [Test]
        public void OuterHullIsFilledAcrossInnerShellsAndOpenSlicesFailClearly()
        {
            var segments = new List<ShipWaterlineTextureBuilder.Segment>();
            Loop(segments, new Vector2(-4,-8), new Vector2(4,-8), new Vector2(4,8), new Vector2(-4,8));
            Loop(segments, new Vector2(-3,-7), new Vector2(3,-7), new Vector2(3,7), new Vector2(-3,7));
            var settings = FlatSettings;
            settings.largestRegionOnly = true;
            var result = ShipWaterlineTextureBuilder.Build(segments, settings);
            try
            {
                Assert.That(Pixel(result, Vector2.zero), Is.LessThan(.02), "Inner shell does not leave a hole over the decks.");
                Assert.That(Pixel(result, new Vector2(3.5f,0)), Is.LessThan(.02));
            }
            finally { Object.DestroyImmediate(result.Texture); }
            Assert.Throws<InvalidOperationException>(() => ShipWaterlineTextureBuilder.Build(Array.Empty<ShipWaterlineTextureBuilder.Segment>(), settings));
            segments.Clear();
            segments.Add(new ShipWaterlineTextureBuilder.Segment(Vector2.zero, Vector2.one));
            Assert.Throws<InvalidOperationException>(() => ShipWaterlineTextureBuilder.Build(segments, settings));
        }

        [Test]
        public void BowRidgeFacesForwardAndClearanceExpandsOnlyTheConfiguredDistance()
        {
            var segments = new List<ShipWaterlineTextureBuilder.Segment>();
            Loop(segments, new Vector2(-3,-8), new Vector2(3,-8), new Vector2(3,8), new Vector2(-3,8));
            var settings = new ShipWaterlineTextureBuilder.Settings { clearance = 1, blend = 1, bowOffset = 1, bowWidth = .7f, gapClosure = 0 };
            var result = ShipWaterlineTextureBuilder.Build(segments, settings);
            try
            {
                Assert.That(Pixel(result, new Vector2(3.5f,0)), Is.LessThan(.02));
                Assert.That(Pixel(result, new Vector2(4.5f,0)), Is.InRange(.38f,.48f));
                Assert.That(Pixel(result, new Vector2(0,9), true), Is.GreaterThan(.9), "Bow ridge is above the hull in texture V.");
                Assert.That(Pixel(result, new Vector2(0,-9), true), Is.Zero);
                Assert.That(result.Texture.GetPixel(0,0).r, Is.EqualTo(128f/255).Within(.005));
            }
            finally { Object.DestroyImmediate(result.Texture); }
        }

        [Test]
        public void MeshSliceUsesWorldTransformsAndFitsTheGeneratedTexture()
        {
            var root = new GameObject("Transformed hull fixture");
            var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
            cube.transform.SetParent(root.transform, false);
            root.transform.SetPositionAndRotation(new Vector3(100,7,200), Quaternion.Euler(0,90,0));
            root.transform.localScale = Vector3.one*1000;
            cube.transform.localPosition = new Vector3(.001f,0,.002f);
            cube.transform.localScale = new Vector3(.006f,.004f,.02f);
            ShipWaterlineTextureBuilder.Result result = null;
            try
            {
                var segments = ShipWaterlineTextureBuilder.Slice(root.transform, null, 7, Vector3.right);
                result = ShipWaterlineTextureBuilder.Build(segments, FlatSettings);
                Assert.That(result.SectionBounds.width, Is.EqualTo(6).Within(.001));
                Assert.That(result.SectionBounds.height, Is.EqualTo(20).Within(.001));
                Assert.That(result.SectionBounds.center.x, Is.EqualTo(1).Within(.001));
                Assert.That(result.SectionBounds.center.y, Is.EqualTo(2).Within(.001));
                var component = root.AddComponent<OceanDeckWaveClamp>();
                component.ConfigureTextures(null, Texture2D.whiteTexture);
                var worldCentre = root.transform.position + Vector3.back + Vector3.right*2;
                component.FitGeneratedHullTexture(result.Texture, worldCentre, Vector3.right, result.SectionBounds.size, result.TextureBounds.size);
                component.GetFootprint(out var centre, out var forward);
                Assert.That(Vector3.Distance(centre,worldCentre), Is.LessThan(.001));
                Assert.That(Vector3.Dot(forward,Vector3.right), Is.GreaterThan(.999));
                var serialized = new SerializedObject(component);
                Assert.AreEqual(Texture2D.whiteTexture, serialized.FindProperty("wakeTexture").objectReferenceValue);
                Assert.AreEqual(result.TextureBounds.size, serialized.FindProperty("textureSizeMetres").vector2Value);
            }
            finally { if (result != null) Object.DestroyImmediate(result.Texture); Object.DestroyImmediate(root); }
        }

        [Test]
        public void WiderBakedTransitionRetainsClampingFartherFromTheHull()
        {
            var segments = new List<ShipWaterlineTextureBuilder.Segment>();
            Loop(segments, new Vector2(-3,-8), new Vector2(3,-8), new Vector2(3,8), new Vector2(-3,8));
            var narrow = ShipWaterlineTextureBuilder.Build(segments, new ShipWaterlineTextureBuilder.Settings
                { clearance = 1.5f, blend = 2, bowWave = false, gapClosure = 0 });
            var wide = ShipWaterlineTextureBuilder.Build(segments, new ShipWaterlineTextureBuilder.Settings
                { clearance = 1.5f, blend = 6, bowWave = false, gapClosure = 0 });
            try
            {
                Assert.That(Pixel(wide, Vector2.zero), Is.LessThan(.01f));
                Assert.That(Pixel(narrow, new Vector2(7,0)), Is.EqualTo(128f/255).Within(.01f));
                Assert.That(Pixel(wide, new Vector2(7,0)), Is.InRange(.35f,.45f), "A wider band must still lower the ceiling where the old map had no effect.");
                var previous = Pixel(wide, new Vector2(4.5f,0));
                for (var x = 4.75f; x <= 10.5f; x += .25f)
                {
                    var value = Pixel(wide, new Vector2(x,0));
                    Assert.That(value, Is.GreaterThanOrEqualTo(previous - .005f));
                    Assert.That(value - previous, Is.LessThan(.08f), "The baked edge must change gradually.");
                    previous = value;
                }
            }
            finally { Object.DestroyImmediate(narrow.Texture); Object.DestroyImmediate(wide.Texture); }
        }

        [Test]
        public void DefaultBlendStartsAtWaterlineAndExtendsOutward()
        {
            var segments = new List<ShipWaterlineTextureBuilder.Segment>();
            Loop(segments, new Vector2(-3,-8), new Vector2(3,-8), new Vector2(3,8), new Vector2(-3,8));
            var settings = new ShipWaterlineTextureBuilder.Settings { bowWave = false, gapClosure = 0 };
            var result = ShipWaterlineTextureBuilder.Build(segments, settings);
            try
            {
                Assert.That(Pixel(result, new Vector2(3,0)), Is.LessThan(.01f), "Full clamp at the waterline.");
                Assert.That(Pixel(result, new Vector2(4.5f,0)), Is.GreaterThan(.01f), "No fully clamped ledge outside the hull.");
                Assert.That(Pixel(result, new Vector2(9,0)), Is.InRange(.42f,.46f), "The convex shoulder should already approach ocean height halfway out.");
                Assert.That(Pixel(result, new Vector2(15.5f,0)), Is.EqualTo(128f/255).Within(.01f));
            }
            finally { Object.DestroyImmediate(result.Texture); }
        }

        [Test]
        public void BowStaysCloseAndShortWhenClampBlendWidens()
        {
            var segments = new List<ShipWaterlineTextureBuilder.Segment>();
            Loop(segments, new Vector2(-3,-8), new Vector2(3,-8), new Vector2(3,8), new Vector2(-3,8));
            var narrow = ShipWaterlineTextureBuilder.Build(segments, new ShipWaterlineTextureBuilder.Settings { blend = 2, gapClosure = 0 });
            var wide = ShipWaterlineTextureBuilder.Build(segments, new ShipWaterlineTextureBuilder.Settings { blend = 12, gapClosure = 0 });
            try
            {
                foreach (var map in new[] { narrow, wide })
                {
                    Assert.That(Pixel(map, new Vector2(0,9), true), Is.GreaterThan(.9), "Ridge stays one metre beyond the bow.");
                    Assert.That(Pixel(map, new Vector2(0,13), true), Is.LessThan(.01), "No long forward extension.");
                    Assert.That(Pixel(map, new Vector2(4,0), true), Is.Zero, "No bow ridge alongside the middle of the hull.");
                    Assert.That(Pixel(map, new Vector2(4,7), true), Is.GreaterThan(.6), "The short ridge still wraps the forward sides.");
                }
                var previousRise = 1f;
                var previousHeight = Pixel(wide, new Vector2(4,0));
                for (var x = 5; x <= 14; x++)
                {
                    var height = Pixel(wide, new Vector2(x,0));
                    var rise = height - previousHeight;
                    Assert.That(rise, Is.GreaterThanOrEqualTo(-.003f));
                    Assert.That(rise, Is.LessThanOrEqualTo(previousRise + .003f), "Outward slope must ease off instead of forming a concave bowl.");
                    previousRise = rise;
                    previousHeight = height;
                }
            }
            finally { Object.DestroyImmediate(narrow.Texture); Object.DestroyImmediate(wide.Texture); }
        }

        [Test]
        public void ImportedShipProducesAnEnclosedRestingWaterline()
        {
            EditorSceneManager.OpenScene("Assets/Scenes/OpenSeaWorld.unity");
            ShipWaterlineTextureBuilder.Result result = null;
            try
            {
                var ship = Object.FindAnyObjectByType<OceanDeckWaveClamp>();
                var saved = ship.HullTexture;
                Assert.IsNotNull(saved);
                var importer = (TextureImporter)AssetImporter.GetAtPath(AssetDatabase.GetAssetPath(saved));
                Assert.AreEqual(TextureImporterShape.Texture2D, importer.textureShape);
                Assert.AreEqual(TextureImporterNPOTScale.None, importer.npotScale);
                Assert.IsFalse(importer.sRGBTexture);
                ship.GetFootprint(out _, out var forward);
                var waterline = ShipWaterlineTextureWindow.EstimateWaterline(ship.transform);
                var segments = ShipWaterlineTextureBuilder.Slice(ship.transform, null, waterline, forward);
                result = ShipWaterlineTextureBuilder.Build(segments, new ShipWaterlineTextureBuilder.Settings());
                Assert.That(result.EnclosedPixels, Is.GreaterThan(100));
                Assert.That(result.SectionBounds.height, Is.InRange(10f,35f));
                Assert.That(result.SectionBounds.width, Is.InRange(2f,10f));
                System.IO.File.WriteAllBytes(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "motu-waterline-generated.png"), result.Texture.EncodeToPNG());
                if (Environment.GetEnvironmentVariable("MOTU_APPLY_WATERLINE_REBAKE") == "1")
                {
                    var baked = ShipWaterlineTextureWindow.SaveAndAssign(ship.transform, result, forward, waterline,
                        new ShipWaterlineTextureBuilder.Settings());
                    EditorSceneManager.SaveScene(ship.gameObject.scene);
                    Debug.Log("MOTU_WATERLINE_ASSET=" + AssetDatabase.GetAssetPath(baked));
                }
                Debug.Log($"Ship waterline Y={waterline:F3}, {segments.Count} segments, {result.Regions} regions, section {result.SectionBounds.size} m, footprint {result.TextureBounds.size} m.");
            }
            finally
            {
                if (result != null) Object.DestroyImmediate(result.Texture);
                EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            }
        }
    }
}
