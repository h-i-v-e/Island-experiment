using System;
using Motu.Settings;
using Motu.World;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Rendering
{
    /// <summary>Bounded, world-space thunder voices that survive their originating storm.</summary>
    internal sealed class SquallThunderAudio : IDisposable
    {
        internal const int MaximumVoices = 48;
        private sealed class Voice
        {
            internal AudioSource Source;
            internal AudioLowPassFilter Filter;
            internal Vector3 Origin;
            internal float Elapsed;
            internal int StormSeed;
            internal uint VariationSeed;
            internal float VisibilityDistance;
            internal bool Pending;
        }

        private readonly SquallAudioProfile profile;
        private readonly Voice[] voices = new Voice[MaximumVoices];
        private readonly AudioClip[] previousClips = new AudioClip[3];
        private readonly Transform parent;
        private readonly AnimationCurve distanceRolloff;
        private AudioListener listener;
        private int createdCount;
        internal int PendingCount { get; private set; }
        internal int PlayedCount { get; private set; }

        internal SquallThunderAudio(Transform parent, SquallAudioProfile profile)
        {
            this.parent = parent;
            this.profile = profile;
            distanceRolloff = profile != null ? CreateDistanceRolloff(profile) : null;
        }

        internal Vector3 ListenerPosition(Vector3 fallback)
        {
            if (listener == null || !listener.isActiveAndEnabled) listener = UnityEngine.Object.FindAnyObjectByType<AudioListener>();
            return listener != null && listener.isActiveAndEnabled ? listener.transform.position : fallback;
        }

        internal bool QueueStrike(WorldSquallState storm, SquallWeatherSettings settings, Vector3 origin,
            Vector3 listenerPosition, uint seed, float ageSinceStrike = 0f)
        {
            if (profile == null || !profile.HasThunder || !storm.Active || storm.Strength <= .001f
                || Vector3.Distance(origin, listenerPosition) >= profile.MaximumDistance) return false;
            var voice = AvailableVoice();
            if (voice == null) return false;
            voice.Origin = origin;
            voice.Elapsed = Mathf.Max(0, ageSinceStrike);
            voice.Pending = true;
            voice.StormSeed = storm.Seed;
            voice.VariationSeed = seed;
            voice.VisibilityDistance = settings.VisibilityDistanceMetres;
            voice.Source.transform.position = origin;
            voice.Source.volume = profile.Volume * Mathf.Clamp01(storm.Strength);
            voice.Source.pitch = .92f + (seed & 0xffffu) / 65535f * .16f;
            PendingCount++;
            return true;
        }

        internal void Advance(float deltaTime, Vector3 listenerPosition,
            ReadOnlySpan<WorldSquallState> liveStorms = default, float visibilityDistance = 0f)
        {
            for (var i = 0; i < createdCount; i++)
            {
                var voice = voices[i];
                voice.Source.transform.position = voice.Origin;
                var distance = Vector3.Distance(voice.Origin, listenerPosition);
                voice.Filter.cutoffFrequency = Mathf.Lerp(10000f, 1200f,
                    Mathf.SmoothStep(0f, 1f, distance / profile.MaximumDistance));
                if (!voice.Pending) continue;
                voice.Elapsed += deltaTime;
                if (distance >= profile.MaximumDistance && voice.Elapsed * profile.SoundSpeedMetresPerSecond >= profile.MaximumDistance)
                {
                    voice.Pending = false;
                    PendingCount--;
                    continue;
                }
                if (voice.Elapsed * profile.SoundSpeedMetresPerSecond < distance) continue;
                voice.Pending = false;
                PendingCount--;
                if (distance >= profile.MaximumDistance) continue;
                var band = BandAtListener(voice, listenerPosition, liveStorms, visibilityDistance, distance);
                var clip = profile.SelectThunder(band, voice.VariationSeed, previousClips[(int)band]);
                if (clip == null) continue;
                voice.Source.clip = clip;
                previousClips[(int)band] = clip;
                voice.Source.Play();
                PlayedCount++;
            }
        }

        private static SquallThunderBand BandAtListener(Voice voice, Vector3 listenerPosition,
            ReadOnlySpan<WorldSquallState> liveStorms, float visibilityDistance, float strikeDistance)
        {
            var range = visibilityDistance > 0f ? visibilityDistance : voice.VisibilityDistance;
            var halfRange = Mathf.Max(1f, range) * .5f;
            foreach (var storm in liveStorms)
            {
                if (!storm.Active || storm.Seed != voice.StormSeed) continue;
                var offset = new Vector2(listenerPosition.x - storm.Center.x, listenerPosition.z - storm.Center.z);
                var radius = storm.Field.RadiusMetres;
                if (offset.sqrMagnitude < radius * radius) return SquallThunderBand.Near;
                return offset.sqrMagnitude >= halfRange * halfRange ? SquallThunderBand.Far : SquallThunderBand.Medium;
            }
            // An emitted sound can outlive its storm, but it must never use a
            // near recording once that storm no longer has an active rain core.
            return strikeDistance >= halfRange ? SquallThunderBand.Far : SquallThunderBand.Medium;
        }

        private Voice AvailableVoice()
        {
            for (var i = 0; i < createdCount; i++)
                if (!voices[i].Pending && !voices[i].Source.isPlaying) return voices[i];
            if (createdCount >= voices.Length) return null;
            var child = new GameObject("Travelling squall thunder") { hideFlags = HideFlags.DontSave };
            child.transform.SetParent(parent, false);
            var source = child.AddComponent<AudioSource>();
            source.playOnAwake = false;
            source.loop = false;
            source.spatialBlend = 1f;
            source.dopplerLevel = 0;
            source.rolloffMode = AudioRolloffMode.Custom;
            source.minDistance = profile.MinimumDistance;
            source.maxDistance = profile.MaximumDistance;
            source.SetCustomCurve(AudioSourceCurveType.CustomRolloff, distanceRolloff);
            source.priority = 96;
            var voice = new Voice { Source = source, Filter = child.AddComponent<AudioLowPassFilter>() };
            voices[createdCount++] = voice;
            return voice;
        }

        private static AnimationCurve CreateDistanceRolloff(SquallAudioProfile profile)
        {
            var range = profile.MaximumDistance;
            var curve = new AnimationCurve(new Keyframe(0f, 1f), new Keyframe(1f, 0f));
            curve.AddKey(new Keyframe(profile.MinimumDistance / range, 1f));
            // Unity scales custom rolloff keys by maxDistance. Keep these anchors
            // in world metres while retaining the full horizon listening range.
            if (profile.MinimumDistance < 1000f && range > 1000f)
                curve.AddKey(new Keyframe(1000f / range, .75f));
            if (profile.MinimumDistance < 2000f && range > 2000f)
                curve.AddKey(new Keyframe(2000f / range, .5f));
            return curve;
        }

        internal void StopAll()
        {
            for (var i = 0; i < createdCount; i++)
            {
                voices[i].Pending = false;
                voices[i].Source.Stop();
            }
            PendingCount = 0;
        }

        public void Dispose()
        {
            StopAll();
            for (var i = 0; i < createdCount; i++) DestroyUnityObject(voices[i].Source.gameObject);
            createdCount = 0;
        }
    }
}
