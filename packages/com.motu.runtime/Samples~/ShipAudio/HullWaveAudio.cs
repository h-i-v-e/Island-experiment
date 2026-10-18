using Motu.Rendering;
using UnityEngine;

// Read after the spray's LateUpdate so sound and spray use the same GPU samples.
[DefaultExecutionOrder(1400)]
[DisallowMultipleComponent]
public sealed class HullWaveAudio : MonoBehaviour
{
    [SerializeField] private OceanHullSpray spray;
    [SerializeField] private Rigidbody hull;
    [SerializeField] private AudioClip[] waveClips;
    [Tooltip("Audible start/end times for each clip, skipping quiet lead-ins and tails.")]
    [SerializeField] private Vector2[] wavePlaybackWindows;
    [Range(0f, 1f)] [SerializeField] private float volume = .7f;
    [Tooltip("Metres above the waterline needed to count as a fresh wave strike.")]
    [Min(.01f)] [SerializeField] private float impactDepth = .12f;
    [Tooltip("The wave must fall below this depth before the same probe can strike again.")]
    [Min(0f)] [SerializeField] private float resetDepth = .02f;
    [Tooltip("Minimum water closing speed relative to the hull, in metres per second.")]
    [Min(0f)] [SerializeField] private float minimumClosingSpeed = .2f;
    [Min(.1f)] [SerializeField] private float minimumImpactInterval = .25f;
    [Tooltip("Relative water speed that produces full volume.")]
    [Min(.1f)] [SerializeField] private float fullVolumeSpeed = 2f;
    [Tooltip("Maximum speed for following a contact along the hull, in metres per second.")]
    [Min(.1f)] [SerializeField] private float maximumContactSpeed = 25f;
    [Min(.01f)] [SerializeField] private float trackingResponseSeconds = .08f;
    [Tooltip("Briefly retain a contact through gaps in the GPU samples, then fade it out.")]
    [Min(0f)] [SerializeField] private float contactHoldSeconds = .3f;
    [Min(.01f)] [SerializeField] private float contactFadeSeconds = .6f;

    private WaveContact[] contacts;
    private SurfaceSample[] samples;
    private readonly WaveVoice[] voices = new WaveVoice[6];
    private float nextImpactTime;
    private int lastClip = -1;

    private void OnEnable()
    {
        if (spray == null) spray = GetComponent<OceanHullSpray>();
        contacts = null;
        samples = null;
        nextImpactTime = Time.time + .5f;
    }

    private void LateUpdate()
    {
        if (spray == null || !spray.isActiveAndEnabled || waveClips == null || waveClips.Length == 0)
        {
            contacts = null;
            FadeVoices(Time.deltaTime);
            return;
        }
        if (contacts == null || contacts.Length != spray.SampleCount)
        {
            contacts = new WaveContact[spray.SampleCount];
            samples = new SurfaceSample[spray.SampleCount];
            StopVoices();
        }

        for (int i = 0; i < samples.Length; i++)
        {
            ref var sample = ref samples[i];
            sample.Valid = spray.TryGetWaterlineSurface(i, out sample.Position, out sample.Depth,
                out sample.WaterVelocity, out sample.WaveScale);
        }
        UpdateVoices(Time.deltaTime);

        float strongest = 0f;
        Vector3 impactPosition = default;
        int impactProbe = -1;
        for (int i = 0; i < contacts.Length; i++)
        {
            var sample = samples[i];
            if (!sample.Valid)
            {
                contacts[i] = default;
                continue;
            }
            var hullVelocity = hull != null ? hull.GetPointVelocity(sample.Position) : Vector3.zero;
            var relative = hullVelocity - sample.WaterVelocity;
            float closingSpeed = Mathf.Max(0f, -relative.y);
            var next = samples[(i + 1) % contacts.Length];
            if (next.Valid)
            {
                var outward = Vector3.ProjectOnPlane(Vector3.Cross(spray.transform.up, next.Position - sample.Position), Vector3.up).normalized;
                closingSpeed += Mathf.Max(0f, Vector3.Dot(relative, outward));
            }
            // Consume each contact even during cooldown so delayed sounds never queue up.
            if (contacts[i].TryImpact(sample.Depth, closingSpeed, impactDepth,
                Mathf.Min(resetDepth, impactDepth * .5f), minimumClosingSpeed)
                && !HasNearbyVoice(i) && closingSpeed > strongest)
            {
                strongest = closingSpeed;
                impactPosition = sample.Position + Vector3.up * sample.Depth;
                impactProbe = i;
            }
        }
        if (strongest <= 0f || Time.time < nextImpactTime || volume <= 0f) return;
        PlayImpact(impactProbe, impactPosition, strongest, samples[impactProbe].WaveScale);
    }

    private void PlayImpact(int probe, Vector3 position, float speed, float waveScale)
    {
        if (waveScale <= 0f) return;
        // Prefer a free slot, then replace the quietest/nearest-finished sound.
        // A full pool must not silently discard a fresh wave strike.
        var selection = new VoiceSelection();
        for (int i = 0; i < voices.Length; i++)
        {
            var source = voices[i].Source;
            bool playing = source != null && source.isPlaying;
            float remaining = playing ? Mathf.Max(0f, voices[i].PlaybackEnd - source.time) : 0f;
            float retention = playing ? source.volume * Mathf.Min(remaining, 1f) : 0f;
            if (playing && Time.time < voices[i].MinimumEndTime) retention += 1f;
            selection.Consider(i, playing, retention);
        }
        ref var voice = ref voices[selection.Index];
        if (voice.Source == null)
        {
            var child = new GameObject("Hull wave sound");
            child.transform.SetParent(transform, false);
            var source = child.AddComponent<AudioSource>();
            source.playOnAwake = false;
            source.spatialBlend = 1f;
            source.dopplerLevel = 0f;
            source.minDistance = 8f;
            source.maxDistance = 60f;
            source.rolloffMode = AudioRolloffMode.Linear;
            voice.Source = source;
        }
        voice.Source.Stop();
        int count = waveClips.Length;
        int index = Random.Range(0, lastClip >= 0 && count > 1 ? count - 1 : count);
        if (count > 1 && lastClip >= 0 && index >= lastClip) index++;
        lastClip = index;
        voice.Coordinate = probe;
        voice.ProbeSpeed = 0f;
        voice.MissingSeconds = 0f;
        voice.BaseVolume = volume * Mathf.Lerp(.55f, 1f, Mathf.Clamp01(speed / fullVolumeSpeed));
        voice.WaveScale = waveScale;
        voice.LocalContact = transform.InverseTransformPoint(position);
        voice.Source.transform.position = position;
        voice.Source.clip = waveClips[index];
        var window = wavePlaybackWindows != null && index < wavePlaybackWindows.Length
            ? wavePlaybackWindows[index] : new Vector2(0f, voice.Source.clip.length);
        float start = Mathf.Clamp(window.x, 0f, Mathf.Max(0f, voice.Source.clip.length - .01f));
        voice.PlaybackEnd = window.y > start ? Mathf.Min(window.y, voice.Source.clip.length) : voice.Source.clip.length;
        voice.MinimumEndTime = Time.time + Mathf.Min(1.5f, voice.PlaybackEnd - start);
        voice.Source.time = start;
        voice.Source.volume = voice.BaseVolume * voice.WaveScale;
        voice.Source.Play();
        nextImpactTime = Time.time + minimumImpactInterval;
    }

    private void OnDisable()
    {
        contacts = null;
        samples = null;
        StopVoices();
    }

    private void StopVoices()
    {
        foreach (var voice in voices)
            if (voice.Source != null) voice.Source.Stop();
    }

    private bool HasNearbyVoice(int probe)
    {
        foreach (var voice in voices)
            if (voice.Source != null && voice.Source.isPlaying && voice.MissingSeconds <= contactHoldSeconds
                && Mathf.Abs(ContactPath.Offset(voice.Coordinate, probe, samples.Length)) <= 4f)
                return true;
        return false;
    }

    private void UpdateVoices(float dt)
    {
        if (dt <= 0f) return;
        for (int i = 0; i < voices.Length; i++)
        {
            ref var voice = ref voices[i];
            if (voice.Source == null || !voice.Source.isPlaying) continue;
            if (voice.Source.time >= voice.PlaybackEnd)
            {
                voice.Source.Stop();
                continue;
            }
            if (!TryFollowFront(voice, dt, out var coordinate, out var position, out var waveScale))
            {
                FadeVoice(ref voice, dt);
                continue;
            }
            // Movement of the wetting edge measures travel along the hull, including
            // the ship's motion through waves. Water particle velocity cannot do this.
            float blend = 1f - Mathf.Exp(-dt / trackingResponseSeconds);
            float speed = ContactPath.Offset(voice.Coordinate, coordinate, samples.Length) / dt;
            voice.ProbeSpeed = Mathf.Lerp(voice.ProbeSpeed, speed, blend);
            voice.Coordinate = coordinate;
            voice.LocalContact = transform.InverseTransformPoint(position);
            voice.MissingSeconds = 0f;
            // Use the shelter from the same samples that move the hull and spray.
            // Apply after the impact-volume floor, including for already playing clips.
            voice.WaveScale = Mathf.Lerp(voice.WaveScale, waveScale, blend);
            voice.Source.volume = voice.BaseVolume * voice.WaveScale;
            voice.Source.transform.position = Vector3.Lerp(voice.Source.transform.position, position, blend);
        }
    }

    private bool TryFollowFront(WaveVoice voice, float dt, out float coordinate, out Vector3 position,
        out float waveScale)
    {
        coordinate = voice.Coordinate;
        position = default;
        waveScale = 0f;
        float bestScore = float.PositiveInfinity;
        float predicted = voice.Coordinate + voice.ProbeSpeed * dt;
        float maximumTravel = maximumContactSpeed * dt + .15f;
        // Stay on neighbouring segments of the closed waterline. A world-space
        // nearest-point search could jump across the bow to the opposite side.
        for (int i = 0; i < samples.Length; i++)
        {
            var a = samples[i];
            var b = samples[(i + 1) % samples.Length];
            if (!a.Valid || !b.Valid || !ContactPath.TryFront(a.Depth, b.Depth, impactDepth, out var t)) continue;
            float candidate = i + t;
            if (Mathf.Abs(ContactPath.Offset(voice.Coordinate, candidate, samples.Length)) > 4f) continue;
            var point = Vector3.Lerp(a.Position, b.Position, t) + Vector3.up * impactDepth;
            if (Vector3.Distance(transform.TransformPoint(voice.LocalContact), point) > maximumTravel) continue;
            float score = Mathf.Abs(ContactPath.Offset(predicted, candidate, samples.Length));
            if (score >= bestScore) continue;
            bestScore = score;
            coordinate = candidate;
            position = point;
            waveScale = Mathf.Lerp(a.WaveScale, b.WaveScale, t);
        }
        return !float.IsPositiveInfinity(bestScore);
    }

    private void FadeVoices(float dt)
    {
        for (int i = 0; i < voices.Length; i++)
            if (voices[i].Source != null && voices[i].Source.isPlaying) FadeVoice(ref voices[i], dt);
    }

    private void FadeVoice(ref WaveVoice voice, float dt)
    {
        voice.MissingSeconds += dt;
        // Losing a geometric edge must not cut off the splash's audible attack.
        float elapsed = Mathf.Min(voice.MissingSeconds - contactHoldSeconds, Time.time - voice.MinimumEndTime);
        float gain = 1f - Mathf.Clamp01(elapsed / contactFadeSeconds);
        voice.Source.volume = voice.BaseVolume * voice.WaveScale * gain;
        if (gain <= 0f) voice.Source.Stop();
    }

    private struct SurfaceSample
    {
        internal Vector3 Position, WaterVelocity;
        internal float Depth, WaveScale;
        internal bool Valid;
    }

    private struct WaveVoice
    {
        internal AudioSource Source;
        internal Vector3 LocalContact;
        internal float Coordinate, ProbeSpeed, MissingSeconds, BaseVolume, WaveScale;
        internal float PlaybackEnd, MinimumEndTime;
    }

    internal struct VoiceSelection
    {
        internal int Index { get; private set; }
        private bool hasFree;
        private float bestRetention;

        internal void Consider(int index, bool playing, float retention)
        {
            if (!playing)
            {
                if (!hasFree) Index = index;
                hasFree = true;
            }
            else if (!hasFree && (index == 0 || retention < bestRetention))
            {
                Index = index;
                bestRetention = retention;
            }
        }
    }

    internal static class ContactPath
    {
        internal static float Offset(float from, float to, int count)
        {
            float delta = (to - from) % count;
            if (delta > count * .5f) delta -= count;
            if (delta < -count * .5f) delta += count;
            return delta;
        }

        internal static bool TryFront(float a, float b, float level, out float fraction)
        {
            fraction = 0f;
            if ((a < level) == (b < level) || a == b) return false;
            fraction = (level - a) / (b - a);
            return true;
        }
    }

    // A fresh wetting event, with hysteresis. Initial/stale samples never invent a strike.
    internal struct WaveContact
    {
        private bool initialized;
        private bool armed;

        internal bool TryImpact(float depth, float closingSpeed, float trigger, float reset, float minimumSpeed)
        {
            if (!initialized)
            {
                initialized = true;
                armed = depth <= reset;
                return false;
            }
            if (depth <= reset) armed = true;
            if (!armed || depth < trigger) return false;
            armed = false;
            return closingSpeed >= minimumSpeed;
        }
    }
}
