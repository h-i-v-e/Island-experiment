using System;
using System.Collections;
using System.Linq;
using Motu.Islands;
using Motu.Rendering;
using Motu.Settings;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class WaterfallEffectsTests
    {
        [Test]
        public void ShadowedMistKeepsSkyLightAndStillDarkensWithTheEnvironment()
        {
            var material = new Material(Shader.Find("Hidden/Motu/Tests/Waterfall Lighting"));
            var target = new RenderTexture(1, 1, 0, RenderTextureFormat.ARGBFloat, RenderTextureReadWrite.Linear);
            var pixel = new Texture2D(1, 1, TextureFormat.RGBAFloat, false, true);
            var oldTarget = RenderTexture.active;
            var ambient = Shader.GetGlobalColor("_MotuCloudAmbientColor");
            var direct = Shader.GetGlobalColor("_MotuCloudLightColor");
            var clouds = Shader.GetGlobalFloat("_MotuCloudEnabled");
            try
            {
                Shader.SetGlobalFloat("_MotuCloudEnabled", 0);
                Shader.SetGlobalColor("_MotuCloudLightColor", Color.white);
                Color Capture(float sky, float visibility)
                {
                    Shader.SetGlobalColor("_MotuCloudAmbientColor", new Color(sky, sky, sky, 1));
                    material.SetFloat("_ProbeShadow", visibility);
                    Graphics.Blit(Texture2D.blackTexture, target, material);
                    RenderTexture.active = target;
                    pixel.ReadPixels(new Rect(0, 0, 1, 1), 0, 0); pixel.Apply();
                    return pixel.GetPixel(0, 0);
                }
                var shade = Capture(.3f, 0);
                var sun = Capture(.3f, 1);
                var night = Capture(.01f, 0);
                Assert.That(shade.r, Is.GreaterThan(.1f), "Daylight mist in terrain shadow must not become black.");
                Assert.That(sun.r, Is.GreaterThan(shade.r * 2f), "Direct sunlight must still be shadowed.");
                Assert.That(night.r, Is.LessThan(shade.r * .1f), "Ambient readability must not introduce an emissive night-time floor.");
                Assert.That(Capture(0, 0).r, Is.Zero.Within(.0001f));
                Assert.IsFalse(ShaderUtil.ShaderHasError(Shader.Find("Motu/Waterfall Foot Mist")));
            }
            finally
            {
                Shader.SetGlobalColor("_MotuCloudAmbientColor", ambient);
                Shader.SetGlobalColor("_MotuCloudLightColor", direct);
                Shader.SetGlobalFloat("_MotuCloudEnabled", clouds);
                RenderTexture.active = oldTarget;
                Object.DestroyImmediate(material); Object.DestroyImmediate(target); Object.DestroyImmediate(pixel);
            }
        }

        sealed class Falls : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Waterfall audio fixture");
            internal readonly WaterfallMistPool Pool;
            internal readonly IslandPreparedWaterfallFoot[] Feet;
            internal readonly WaterfallFootIndex Index;
            readonly WaterfallAudioProfile profile;
            readonly AudioClip clip;
            internal Falls(int count, float? uniformDrop = null, float baseHeight = 2f, float? uniformHalfWidth = null)
            {
                clip = AudioClip.Create("Synthetic waterfall loop", 44100, 1, 44100, false);
                profile = ScriptableObject.CreateInstance<WaterfallAudioProfile>();
                var serialized = new SerializedObject(profile);
                serialized.FindProperty("loop").objectReferenceValue = clip;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                Pool = Root.AddComponent<WaterfallMistPool>();
                serialized = new SerializedObject(Pool);
                serialized.FindProperty("audioProfile").objectReferenceValue = profile;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                Feet = new IslandPreparedWaterfallFoot[count];
                for (int i = 0; i < count; i++)
                    Feet[i] = new IslandPreparedWaterfallFoot(new Vector3(i * 2f - 10, baseHeight, 0), Vector3.forward,
                        uniformHalfWidth ?? (i == 0 ? .5f : 5f), uniformDrop ?? (i == 0 ? .5f : 8f));
                Index = new WaterfallFootIndex(Feet, 2000);
                Pool.Initialize(Feet, 2000, true);
            }
            internal void Focus(Vector3 position) => Pool.SetPlayerPosition(position, Index.CellAt(position));
            internal AudioSource[] Sources => Root.GetComponentsInChildren<AudioSource>(true);
            internal AudioSource[] Playing => Sources.Where(source => source.isPlaying).ToArray();
            internal ParticleSystem[] Sprays => Root.GetComponentsInChildren<ParticleSystem>(true);
            public void Dispose()
            {
                Object.DestroyImmediate(Root); Object.DestroyImmediate(profile); Object.DestroyImmediate(clip);
            }
        }

        [Test]
        public void EachNearbyFallLoopsAtItsOwnTranslatedImpactAndLargerFallsAreLouder()
        {
            using var falls = new Falls(2);
            falls.Root.transform.SetPositionAndRotation(new Vector3(123, 10, -71), Quaternion.Euler(0, 32, 0));
            falls.Focus(Vector3.zero);
            var playing = falls.Playing;
            Assert.That(playing.Length, Is.EqualTo(2));
            // Slots are assigned nearest-first, independently of export order.
            playing = playing.OrderBy(source => source.transform.localPosition.x).ToArray();
            for (int i = 0; i < playing.Length; i++)
            {
                var source = playing[i];
                Assert.IsTrue(source.loop); Assert.IsFalse(source.playOnAwake);
                Assert.That(source.spatialBlend, Is.EqualTo(1)); Assert.That(source.dopplerLevel, Is.Zero);
                Assert.That(source.rolloffMode, Is.EqualTo(AudioRolloffMode.Linear));
                Assert.That(source.minDistance, Is.GreaterThan(0)); Assert.That(source.maxDistance, Is.LessThan(180));
                Assert.That(Vector3.Distance(source.transform.position, falls.Root.transform.TransformPoint(falls.Feet[i].position)), Is.LessThan(.001f));
            }
            Assert.That(playing[1].volume, Is.GreaterThan(playing[0].volume));
            Assert.That(playing[0].volume, Is.InRange(0, .65f)); Assert.That(playing[1].volume, Is.InRange(0, .65f));
            Assert.That(playing[1].time, Is.GreaterThan(.5f), "Neighbouring falls should not play in phase.");
            var originalSources = playing;
            playing[0].time = .3f;
            falls.Focus(Vector3.right);
            Assert.That(falls.Playing, Is.EquivalentTo(originalSources));
            Assert.That(playing[0].time, Is.GreaterThanOrEqualTo(.29f), "Small player movements must not restart the loop.");
        }

        [UnityTest]
        public IEnumerator UnloadingHidingAndDisablingFallsStopLoopsAndAllowThemToResume()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            using var falls = new Falls(2);
            falls.Focus(Vector3.zero); Assert.That(falls.Playing.Length, Is.EqualTo(2));
            Assert.That(falls.Sprays, Is.Empty);
            falls.Focus(Vector3.right * 800); Assert.That(falls.Playing, Is.Empty);
            Assert.IsFalse(falls.Sprays.Any(spray => spray.isPlaying));
            falls.Focus(Vector3.zero); Assert.That(falls.Playing.Length, Is.EqualTo(2));
            falls.Pool.SetRiversVisible(false); Assert.That(falls.Playing, Is.Empty);
            falls.Pool.SetRiversVisible(true); falls.Focus(Vector3.zero); Assert.That(falls.Playing.Length, Is.EqualTo(2));
            falls.Pool.enabled = false; Assert.That(falls.Playing, Is.Empty);
            Assert.IsFalse(falls.Sprays.Any(spray => spray.isPlaying));
            falls.Focus(Vector3.zero); Assert.That(falls.Playing, Is.Empty, "Streaming updates must not restart a disabled pool.");
            falls.Pool.enabled = true; falls.Focus(Vector3.zero); Assert.That(falls.Playing.Length, Is.EqualTo(2));
            falls.Root.SetActive(false); Assert.That(falls.Playing, Is.Empty);
            falls.Root.SetActive(true); falls.Focus(Vector3.zero); Assert.That(falls.Playing.Length, Is.EqualTo(2));
            falls.Pool.DisposePool();
            yield return null;
            Assert.That(falls.Sources, Is.Empty);
            Assert.That(falls.Sprays, Is.Empty);
        }

        [UnityTearDown]
        public IEnumerator ReturnToEditMode()
        {
            if (Application.isPlaying) yield return new ExitPlayMode();
        }

        [Test]
        public void DenseWaterfallsReuseABoundedPoolOfAudioSources()
        {
            using var falls = new Falls(64);
            falls.Focus(Vector3.zero);
            Assert.That(falls.Sources.Length, Is.EqualTo(32)); Assert.That(falls.Playing.Length, Is.EqualTo(32));
            var originalSources = falls.Sources;
            falls.Focus(Vector3.right * 50);
            Assert.That(falls.Sources, Is.EqualTo(originalSources));
            Assert.That(falls.Playing.Length, Is.EqualTo(32));
            Assert.That(falls.Playing.Select(source => source.transform.position).Distinct().Count(), Is.EqualTo(32));
            Assert.That(falls.Sprays, Is.Empty);
        }

        [TestCase(2f, 8f, .5f, 0f)]
        [TestCase(0f, 8f, 10f, 0f)]
        [TestCase(-3f, 6f, .5f, 1.25f)]
        [TestCase(-1.25f, 8f, 5f, .625f)]
        [TestCase(-6.75f, 8f, 10f, 1.875f)]
        public void FullWidthMistAndAudioRemainAtTheActualFaceContactWithoutParticles(
            float baseHeight, float drop, float halfWidth, float upstreamOffset)
        {
            using var falls = new Falls(1, drop, baseHeight, halfWidth);
            falls.Root.transform.SetPositionAndRotation(new Vector3(123, 10, -71), Quaternion.Euler(0, 73, 0));
            var foot = falls.Feet[0];
            var contact = foot.position - foot.direction * upstreamOffset;
            contact.y = Mathf.Max(baseHeight, 0f);
            falls.Focus(contact);
            Assert.That(falls.Sprays, Is.Empty, "Waterfalls must not create any particle emitters.");
            var audio = falls.Playing.Single();
            var worldContact = falls.Root.transform.TransformPoint(contact);
            Assert.That(Vector3.Distance(audio.transform.position, worldContact), Is.LessThan(.001f));
            var mist = falls.Root.GetComponentsInChildren<MeshRenderer>().Single(renderer => renderer.enabled);
            Assert.That(Vector3.Distance(mist.transform.parent.position, worldContact), Is.LessThan(.001f),
                "Submerged falls must retain their corrected curved-face sea-plane intersection.");
            Assert.That(mist.transform.localScale.x, Is.EqualTo(Mathf.Clamp(halfWidth * 2.7f, 3f, 24f)).Within(.0001f));
        }

        [TestCase(-5f, 2f)]
        [TestCase(-5f, 5f)]
        public void FullySubmergedFallsHaveNoFalseSurfaceEffects(float baseHeight, float drop)
        {
            using var falls = new Falls(1, drop, baseHeight);
            falls.Focus(new Vector3(-10, 0, 0));
            Assert.IsFalse(falls.Sprays.Any(system => system.isPlaying));
            Assert.That(falls.Playing, Is.Empty);
            Assert.IsFalse(falls.Root.GetComponentsInChildren<MeshRenderer>().Any(renderer => renderer.enabled));
        }
    }
}
