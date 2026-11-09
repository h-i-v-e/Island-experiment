using System;
using System.Collections;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using Motu.Rendering;
using Motu.Settings;
using Motu.World;
using NUnit.Framework;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.TestTools;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public sealed class SquallThunderTests
    {
        private sealed class Fixture : IDisposable
        {
            internal readonly GameObject Root = new GameObject("Thunder fixture");
            internal readonly SquallAudioProfile Profile = ScriptableObject.CreateInstance<SquallAudioProfile>();
            internal readonly SquallThunderAudio Thunder;
            internal readonly AudioClip Clip;
            private readonly List<AudioClip> bankClips = new List<AudioClip>();
            internal Fixture()
            {
                Clip = TestClip();
                var serialized = new SerializedObject(Profile);
                serialized.FindProperty("thunder").objectReferenceValue = Clip;
                serialized.ApplyModifiedPropertiesWithoutUndo();
                Thunder = new SquallThunderAudio(Root.transform, Profile);
            }
            internal bool Queue(Vector3 origin, Vector3 listener, uint seed = 7) =>
                Thunder.QueueStrike(Storm(origin), SquallWeatherSettings.Default, origin, listener, seed);

            internal void ConfigureBanks(int variants = 1)
            {
                var serialized = new SerializedObject(Profile);
                foreach (var name in new[] { "nearThunder", "mediumThunder", "farThunder" })
                {
                    var array = serialized.FindProperty(name);
                    array.arraySize = variants;
                    for (var i = 0; i < variants; i++)
                    {
                        var clip = AudioClip.Create(name + i, 4410, 1, 44100, false);
                        bankClips.Add(clip);
                        array.GetArrayElementAtIndex(i).objectReferenceValue = clip;
                    }
                }
                serialized.ApplyModifiedPropertiesWithoutUndo();
            }
            public void Dispose()
            {
                Thunder.Dispose(); Object.DestroyImmediate(Root);
                Object.DestroyImmediate(Profile); Object.DestroyImmediate(Clip);
                foreach (var clip in bankClips) Object.DestroyImmediate(clip);
            }
        }

        private static WorldSquallState Storm(Vector3 center, int seed = 7129) =>
            new WorldSquallState(center, 500, 1, 1, seed,
                new WorldSquallField(new Vector2(center.x, center.z), 250, .25f, 4, 1, Vector2.right * 36), 4);

        private static AudioClip TestClip()
        {
            var path = Environment.GetEnvironmentVariable("MOTU_THUNDER_TEST_WAV");
            if (!string.IsNullOrEmpty(path) && File.Exists(path)) return ReadMonoPcm(path);
            var clip = AudioClip.Create("Thunder test tone", 44100, 1, 44100, false);
            var samples = new float[44100];
            for (var i = 0; i < samples.Length; i++) samples[i] = .05f * Mathf.Sin(i * 2f * Mathf.PI * 130f / 44100f);
            clip.SetData(samples, 0); return clip;
        }

        private static AudioClip ReadMonoPcm(string path)
        {
            using var reader = new BinaryReader(File.OpenRead(path));
            reader.BaseStream.Position = 12;
            while (reader.BaseStream.Position + 8 <= reader.BaseStream.Length)
            {
                var kind = new string(reader.ReadChars(4));
                var size = reader.ReadInt32();
                if (kind != "data") { reader.BaseStream.Position += size + (size & 1); continue; }
                var samples = new float[size / 2];
                for (var i = 0; i < samples.Length; i++) samples[i] = reader.ReadInt16() / 32768f;
                var clip = AudioClip.Create("Actual authored squall thunder", samples.Length, 1, 44100, false);
                clip.SetData(samples, 0); return clip;
            }
            throw new InvalidDataException("The thunder fixture has no PCM data.");
        }

        [Test]
        public void SoundWaitsForTheWavefrontAndAccountsForListenerMovement()
        {
            using var fixture = new Fixture();
            var thunder = fixture.Thunder;
            Assert.That(fixture.Queue(Vector3.forward * 4000, Vector3.zero), Is.False,
                "Thunder at the silent distance must not consume a voice.");
            Assert.That(fixture.Queue(Vector3.right * 343, Vector3.zero), Is.True);
            thunder.Advance(.5f, Vector3.zero);
            Assert.That(thunder.PlayedCount, Is.Zero);
            thunder.Advance(.5f, Vector3.left * 343);
            Assert.That(thunder.PlayedCount, Is.Zero, "Moving away extends the travel time.");
            thunder.Advance(1f, Vector3.left * 343);
            Assert.That(thunder.PlayedCount, Is.EqualTo(1));
            Assert.That(thunder.PendingCount, Is.Zero);
            var source = fixture.Root.GetComponentInChildren<AudioSource>();
            Assert.That(source.transform.position, Is.EqualTo(Vector3.right * 343));
            Assert.That(source.spatialBlend, Is.EqualTo(1));
            Assert.That(source.maxDistance, Is.EqualTo(4000));
            fixture.Root.transform.position = Vector3.forward * 100;
            thunder.Advance(0, Vector3.zero);
            Assert.That(source.transform.position, Is.EqualTo(Vector3.right * 343), "The sound stays at its emission point.");
        }

        [Test]
        public void EachStrikeQueuesOnceAndThunderSurvivesVisualCullingAndStormRetirement()
        {
            using var fixture = new Fixture();
            using var visuals = new SquallVisualRenderer(fixture.Root.transform);
            var settings = SquallWeatherSettings.Default;
            settings.MinimumSpeedMultiplier = settings.MaximumSpeedMultiplier = 4;
            settings.LightningIntervalSeconds = .2f;
            settings.VisibilityDistanceMetres = 2000;
            var center = Vector3.forward * 3000;
            WorldSquallState State(float age) => new WorldSquallState(center, 500, age, 1, 7129,
                new WorldSquallField(new Vector2(0, 3000), 250, .25f, 4, 1, Vector2.up * 36), 4);
            visuals.UpdateVisuals(State(0), settings, Vector3.zero, 0, fixture.Thunder);
            visuals.UpdateVisuals(State(1), settings, Vector3.zero, 1, fixture.Thunder);
            for (var i = 0; i < 10; i++) visuals.UpdateVisuals(State(1), settings, Vector3.zero, 0, fixture.Thunder);
            Assert.That(visuals.Visible, Is.False);
            Assert.That(fixture.Thunder.PendingCount, Is.EqualTo(1), "Flash sub-pulses and repeated render updates must not duplicate thunder.");
            visuals.UpdateVisuals(default, settings, Vector3.zero, 0, fixture.Thunder);
            fixture.Thunder.Advance(16f, Vector3.zero);
            Assert.That(fixture.Thunder.PlayedCount, Is.EqualTo(1), "An emitted sound wave outlives the storm.");
        }

        [Test]
        public void VoicePoolIsBoundedAndOwnerReleaseCancelsPendingAndPlayingThunder()
        {
            using var fixture = new Fixture();
            for (var i = 0; i < SquallThunderAudio.MaximumVoices; i++)
                Assert.That(fixture.Queue(Vector3.forward * 1000, Vector3.zero, (uint)i), Is.True);
            Assert.That(fixture.Queue(Vector3.forward * 1000, Vector3.zero, 99), Is.False);
            Assert.That(fixture.Root.GetComponentsInChildren<AudioSource>().Length, Is.EqualTo(SquallThunderAudio.MaximumVoices));
            fixture.Thunder.StopAll();
            Assert.That(fixture.Thunder.PendingCount, Is.Zero);
            Assert.That(fixture.Root.GetComponentsInChildren<AudioSource>().Any(source => source.isPlaying), Is.False);
        }

        [UnityTest]
        public IEnumerator ActualThunderIsSilentBeforeArrivalAndAudibleThroughTheNativeMixerAfterwards()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var listener = new GameObject("Thunder listener"); listener.AddComponent<AudioListener>();
            var oldPause = AudioListener.pause; var oldVolume = AudioListener.volume;
            using var fixture = new Fixture();
            try
            {
                AudioListener.pause = false; AudioListener.volume = 1;
                Assert.That(fixture.Thunder.ListenerPosition(Vector3.one), Is.EqualTo(listener.transform.position));
                fixture.Queue(Vector3.forward * 343, Vector3.zero);
                fixture.Thunder.Advance(.9f, Vector3.zero);
                yield return new WaitForSecondsRealtime(.1f);
                Assert.That(fixture.Root.GetComponentsInChildren<AudioSource>().Any(source => source.isPlaying), Is.False);
                fixture.Thunder.Advance(.1f, Vector3.zero);
                var samples = new float[512]; var peak = 0f;
                var until = Time.realtimeSinceStartup + .7f;
                while (Time.realtimeSinceStartup < until)
                {
                    AudioListener.GetOutputData(samples, 0);
                    foreach (var value in samples) peak = Mathf.Max(peak, Mathf.Abs(value));
                    yield return null;
                }
                Assert.That(fixture.Thunder.PlayedCount, Is.EqualTo(1));
                Assert.That(peak, Is.GreaterThan(.0001f), "The delayed spatial source must reach the actual listener.");
                var directory = "/tmp/squall-thunder-waterfalls-20261109"; Directory.CreateDirectory(directory);
                File.WriteAllText(Path.Combine(directory, "thunder-native-mixer.txt"),
                    $"Clip: {fixture.Clip.name}; peak listener sample: {peak}; arrival: 343m / 343m/s = 1s\n");
            }
            finally { AudioListener.pause = oldPause; AudioListener.volume = oldVolume; Object.DestroyImmediate(listener); }
        }

        [UnityTest]
        public IEnumerator SpatialRolloffReachesRequestedGainsThroughTheNativeMixer()
        {
            EditorSceneManager.NewScene(NewSceneSetup.EmptyScene);
            yield return new EnterPlayMode();
            var listener = new GameObject("Attenuation listener"); listener.AddComponent<AudioListener>();
            var oldPause = AudioListener.pause; var oldVolume = AudioListener.volume;
            using var fixture = new Fixture();
            var tone = AudioClip.Create("Steady attenuation reference", 44100, 1, 44100, false);
            var wave = new float[44100];
            for (var i = 0; i < wave.Length; i++) wave[i] = .05f * Mathf.Sin(i * 2f * Mathf.PI * 130f / 44100f);
            tone.SetData(wave, 0);
            try
            {
                AudioListener.pause = false; AudioListener.volume = 1;
                fixture.Queue(Vector3.zero, Vector3.zero); fixture.Thunder.Advance(0, Vector3.zero);
                var source = fixture.Root.GetComponentInChildren<AudioSource>();
                Assert.That(source.rolloffMode, Is.EqualTo(AudioRolloffMode.Custom));
                Assert.That(source.maxDistance, Is.EqualTo(4000));
                source.clip = tone; source.pitch = 1; source.loop = true; source.Play();
                var distances = new[] { 0f, 1000f, 2000f, 4000f };
                var rms = new float[distances.Length];
                for (var i = 0; i < distances.Length; i++)
                {
                    listener.transform.position = Vector3.back * distances[i];
                    fixture.Thunder.Advance(0, listener.transform.position);
                    yield return MeasureListenerRms(value => rms[i] = value);
                }
                Assert.That(rms[0], Is.GreaterThan(.001f));
                Assert.That(rms[1] / rms[0], Is.EqualTo(.75f).Within(.04f));
                Assert.That(rms[2] / rms[0], Is.EqualTo(.5f).Within(.04f));
                Assert.That(rms[3], Is.LessThan(.0001f));
                File.WriteAllText(Path.Combine(Path.GetTempPath(), "squall-attenuation-mixer.txt"),
                    $"RMS at 0m, 1km, 2km, 4km: {string.Join(", ", rms)}\nGains: {rms[1] / rms[0]}, {rms[2] / rms[0]}\n");
            }
            finally
            {
                fixture.Thunder.StopAll(); Object.DestroyImmediate(tone); Object.DestroyImmediate(listener);
                AudioListener.pause = oldPause; AudioListener.volume = oldVolume;
            }
        }

        private static IEnumerator MeasureListenerRms(Action<float> result)
        {
            yield return new WaitForSecondsRealtime(.15f);
            var samples = new float[512]; double sum = 0; var count = 0;
            var until = Time.realtimeSinceStartup + .25f;
            while (Time.realtimeSinceStartup < until)
            {
                AudioListener.GetOutputData(samples, 0);
                foreach (var sample in samples) sum += sample * sample;
                count += samples.Length;
                yield return null;
            }
            result(Mathf.Sqrt((float)(sum / count)));
        }

        [TestCase(0f, "nearThunder")]
        [TestCase(249f, "nearThunder")]
        [TestCase(250f, "mediumThunder")]
        [TestCase(1999f, "mediumThunder")]
        [TestCase(2000f, "farThunder")]
        [TestCase(3999f, "farThunder")]
        public void BanksFollowTheRainCoreAndTheFarHalfOfLiveVisibility(float distance, string expected)
        {
            using var fixture = new Fixture();
            fixture.ConfigureBanks();
            var storm = Storm(Vector3.zero);
            var listener = Vector3.right * distance;
            Assert.That(fixture.Thunder.QueueStrike(storm, SquallWeatherSettings.Default,
                Vector3.zero, listener, 7), Is.True);
            fixture.Thunder.Advance(30, listener, new[] { storm }, 4000);
            Assert.That(fixture.Root.GetComponentInChildren<AudioSource>().clip.name, Does.StartWith(expected));
        }

        [Test]
        public void EnteringTheEmittingStormBeforeArrivalSelectsNearFromTheLiveFootprint()
        {
            using var fixture = new Fixture();
            fixture.ConfigureBanks();
            var storm = Storm(Vector3.right * 2000);
            fixture.Thunder.QueueStrike(storm, SquallWeatherSettings.Default, storm.Center, Vector3.zero, 7);
            fixture.Thunder.Advance(1, Vector3.zero, new[] { storm }, 4000);
            var source = fixture.Root.GetComponentInChildren<AudioSource>();
            Assert.That(source.clip, Is.Null, "A recording is chosen when the wave reaches the player.");
            storm = Storm(Vector3.zero);
            fixture.Thunder.Advance(5, Vector3.zero, new[] { storm }, 4000);
            Assert.That(source.clip.name, Does.StartWith("nearThunder"));
            Assert.That(source.transform.position, Is.EqualTo(Vector3.right * 2000));
        }

        [Test]
        public void NearIsReservedForTheEmittingStormAndNeverUsedAfterThatStormRetires()
        {
            using var fixture = new Fixture();
            fixture.ConfigureBanks();
            var storm = Storm(Vector3.right * 1000);
            var other = Storm(Vector3.zero, 123);
            fixture.Thunder.QueueStrike(storm, SquallWeatherSettings.Default, storm.Center, Vector3.zero, 7);
            fixture.Thunder.Advance(4, Vector3.zero, new[] { storm, other }, 4000);
            Assert.That(fixture.Root.GetComponentsInChildren<AudioSource>().Last().clip.name, Does.StartWith("mediumThunder"));
            fixture.Thunder.StopAll();
            storm = Storm(Vector3.zero);
            fixture.Thunder.QueueStrike(storm, SquallWeatherSettings.Default, storm.Center, Vector3.zero, 7);
            fixture.Thunder.Advance(0, Vector3.zero, Array.Empty<WorldSquallState>(), 4000);
            Assert.That(fixture.Root.GetComponentsInChildren<AudioSource>().First().clip.name, Does.StartWith("mediumThunder"));
        }

        [Test]
        public void FarThresholdUsesLiveVisibilityAndLiveStormCentreInsteadOfHearingRangeOrBoltJitter()
        {
            using var fixture = new Fixture();
            fixture.ConfigureBanks();
            var storm = Storm(Vector3.right * 1900);
            fixture.Thunder.QueueStrike(storm, SquallWeatherSettings.Default, Vector3.right * 2050, Vector3.zero, 7);
            fixture.Thunder.Advance(7, Vector3.zero, new[] { storm }, 4000);
            var source = fixture.Root.GetComponentInChildren<AudioSource>();
            Assert.That(source.clip.name, Does.StartWith("mediumThunder"), "A fork at2050m must not move a1900m storm into the far band.");
            fixture.Thunder.StopAll();
            fixture.Thunder.QueueStrike(storm, SquallWeatherSettings.Default, storm.Center, Vector3.zero, 7);
            fixture.Thunder.Advance(7, Vector3.zero, new[] { storm }, 3000);
            Assert.That(source.clip.name, Does.StartWith("farThunder"), "The boundary follows half the live visibility range.");
        }

        [Test]
        public void EveryBankVariesClipsWithoutImmediateRepeatsOrConsumingUnityRandom()
        {
            using var fixture = new Fixture();
            fixture.ConfigureBanks(4);
            var random = UnityEngine.Random.state;
            foreach (var band in new[] { SquallThunderBand.Near, SquallThunderBand.Medium, SquallThunderBand.Far })
            {
                AudioClip previous = null;
                var seen = new HashSet<AudioClip>();
                for (uint seed = 0; seed < 12; seed++)
                {
                    var clip = fixture.Profile.SelectThunder(band, seed, previous);
                    Assert.That(clip, Is.Not.SameAs(previous));
                    Assert.That(clip.name, Does.StartWith(band.ToString().ToLowerInvariant() + "Thunder"));
                    seen.Add(clip); previous = clip;
                }
                Assert.That(seen.Count, Is.EqualTo(4));
            }
            Assert.That(UnityEngine.Random.state, Is.EqualTo(random));
        }

        [UnityTearDown]
        public IEnumerator ReturnToEditMode()
        {
            if (Application.isPlaying) yield return new ExitPlayMode();
        }
    }
}
