using Motu.World;
using UnityEngine;

[DefaultExecutionOrder(1450)]
[DisallowMultipleComponent]
public sealed class ShoreWaveAudio : MonoBehaviour
{
    [SerializeField] private Transform listener;
    [SerializeField] private AudioClip[] waveClips;
    [SerializeField] private Vector2[] playbackWindows;
    [Range(1, 32)] [SerializeField] private int maximumProbes = 12;
    [Range(8f, 128f)] [SerializeField] private float searchRadius = 80f;
    [Min(.2f)] [SerializeField] private float placementInterval = 1f;
    [Min(1f)] [SerializeField] private float probeSpacing = 12f;
    [Range(0f, 1f)] [SerializeField] private float maximumVolume = .65f;
    [Tooltip("Local shore-wave amplitude in metres that reaches Maximum Volume (roughly half the wave height).")]
    [Min(.01f)] [SerializeField] private float fullVolumeWaveAmplitude = .5f;

    private OceanSurfaceController ocean;
    private OceanShoreSampler placement, waves;
    private Probe[] probes;
    private Vector3[] positions;
    private readonly bool[] usedCandidates = new bool[48];
    private readonly Voice[] voices = new Voice[6];
    private Vector3 placementCentre, previousListener;
    private float nextPlacement, nextSample, nextSearch;
    private int placementRevision, waveRevision, lastClip = -1;
    private bool haveListenerPosition;

    private struct Probe
    {
        internal Vector3 Position;
        internal bool Active, HasSample, Breaking;
    }
    private struct Voice
    {
        internal AudioSource Source;
        internal Vector3 Position;
        internal float End, Started;
    }

    private void OnEnable()
    {
        if (listener == null)
        {
            var camera = GetComponentInChildren<Camera>(true);
            if (camera != null) listener = camera.transform;
        }
        nextPlacement = nextSample = nextSearch = 0f;
        haveListenerPosition = false;
    }

    private void LateUpdate()
    {
        UpdateVoices(Time.deltaTime);
        if (listener == null || waveClips == null || waveClips.Length == 0) return;
        if (ocean == null || !ocean.isActiveAndEnabled)
        {
            if (Time.time < nextSearch) return;
            nextSearch = Time.time + 1f;
            ocean = FindAnyObjectByType<OceanSurfaceController>();
            if (ocean == null || !ocean.isActiveAndEnabled) return;
        }
        if (waves == null || probes.Length != Mathf.Clamp(maximumProbes, 1, 32))
        {
            ReleaseQueries();
            if (!OceanShoreSampler.Supported) { enabled = false; return; }
            int count = Mathf.Clamp(maximumProbes, 1, 32);
            try
            {
                placement = new OceanShoreSampler(usedCandidates.Length);
                waves = new OceanShoreSampler(count);
            }
            catch (System.NotSupportedException error)
            {
                ReleaseQueries();
                Debug.LogWarning(error.Message, this);
                enabled = false;
                return;
            }
            probes = new Probe[count]; positions = new Vector3[count];
            placementRevision = waveRevision = 0;
            nextPlacement = nextSample = 0f;
        }
        if (haveListenerPosition && HorizontalSquared(listener.position, previousListener) > searchRadius * searchRadius)
        {
            System.Array.Clear(probes, 0, probes.Length);
            StopVoices(); nextPlacement = 0f;
        }
        previousListener = listener.position; haveListenerPosition = true;
        if (placement.Revision != placementRevision)
        {
            placementRevision = placement.Revision;
            if (HorizontalSquared(listener.position, placementCentre) < searchRadius * searchRadius * .25f)
                PlaceProbes();
            else nextPlacement = 0f;
        }
        if (waves.Revision != waveRevision)
        {
            waveRevision = waves.Revision;
            for (int i = 0; i < probes.Length; i++)
            {
                ref var probe = ref probes[i];
                if (!probe.Active) continue;
                if (!waves.TryGetSample(i, out var sample)
                    || HorizontalSquared(sample.Position, probe.Position) > .25f)
                { probe.HasSample = false; continue; }
                if (CrossedBreaker(ref probe, sample.Breaking)) PlayWave(sample);
            }
        }
        if (Time.time >= nextPlacement && placement.SubmitPlacement(ocean, listener.position, searchRadius))
        {
            placementCentre = listener.position;
            nextPlacement = Time.time + Mathf.Max(.2f, placementInterval);
        }
        if (Time.time >= nextSample)
        {
            for (int i = 0; i < probes.Length; i++) positions[i] = probes[i].Active ? probes[i].Position : listener.position;
            if (waves.SubmitWaves(ocean, positions)) nextSample = Time.time + .05f;
        }
    }

    private static bool CrossedBreaker(ref Probe probe, float breaking)
    {
        // Seed without firing when a probe is first placed in an existing crest.
        if (!probe.HasSample) { probe.HasSample = true; probe.Breaking = breaking >= .08f; return false; }
        if (breaking <= .02f) probe.Breaking = false;
        if (probe.Breaking || breaking < .08f) return false;
        probe.Breaking = true;
        return true;
    }

    private void PlaceProbes()
    {
        System.Array.Clear(usedCandidates, 0, usedCandidates.Length);
        // Keep existing world positions when the contour is still nearby. A
        // moving camera must not drag the probes or restart a passing crest.
        for (int i = 0; i < probes.Length; i++)
        {
            if (!probes[i].Active) continue;
            int nearest = -1; float distance = 16f;
            for (int c = 0; c < usedCandidates.Length; c++)
                if (!usedCandidates[c] && placement.TryGetSample(c, out var candidate))
                {
                    float d = HorizontalSquared(candidate.Position, probes[i].Position);
                    if (d < distance) { distance = d; nearest = c; }
                }
            if (nearest >= 0 && HorizontalSquared(probes[i].Position, listener.position) <= searchRadius * searchRadius)
                usedCandidates[nearest] = true;
            else probes[i] = default;
        }
        for (int i = 0; i < probes.Length; i++)
        {
            if (probes[i].Active) continue;
            int best = -1; float bestDistance = float.PositiveInfinity;
            for (int c = 0; c < usedCandidates.Length; c++)
            {
                if (usedCandidates[c] || !placement.TryGetSample(c, out var candidate)) continue;
                float distance = HorizontalSquared(candidate.Position, listener.position);
                if (distance > searchRadius * searchRadius) continue;
                // Prefer the configured depth contour; lower-confidence samples
                // are useful when a gently sloping beach needs shallower probes.
                float score = distance + (1f - Mathf.Clamp01(candidate.Influence)) * searchRadius * searchRadius * 4f;
                if (score >= bestDistance) continue;
                bool separated = true;
                foreach (var probe in probes)
                    if (probe.Active && HorizontalSquared(probe.Position, candidate.Position) < probeSpacing * probeSpacing)
                    { separated = false; break; }
                if (!separated) continue;
                best = c; bestDistance = score;
            }
            if (best < 0) break;
            placement.TryGetSample(best, out var selected);
            probes[i] = new Probe { Active = true, Position = selected.Position };
            usedCandidates[best] = true;
        }
    }

    private void PlayWave(OceanShoreSampler.Sample sample)
    {
        float volume = WaveVolume(sample);
        if (volume <= 0f) return;
        foreach (var voice in voices)
            if (voice.Source != null && voice.Source.isPlaying && Time.time - voice.Started < .8f
                && HorizontalSquared(voice.Position, sample.Position) < probeSpacing * probeSpacing) return;
        int clip = -1, choices = 0;
        for (int i = 0; i < waveClips.Length; i++)
            if (waveClips[i] != null && i != lastClip && Random.Range(0, ++choices) == 0) clip = i;
        if (clip < 0 && lastClip >= 0 && lastClip < waveClips.Length && waveClips[lastClip] != null) clip = lastClip;
        if (clip < 0) return;
        int selected = 0; float leastRetention = float.PositiveInfinity;
        for (int i = 0; i < voices.Length; i++)
        {
            var source = voices[i].Source;
            if (source == null || !source.isPlaying) { selected = i; break; }
            float retention = source.volume * Mathf.Min(1f, Mathf.Max(0f, voices[i].End - source.time));
            if (retention < leastRetention) { leastRetention = retention; selected = i; }
        }
        ref var target = ref voices[selected];
        if (target.Source == null)
        {
            var child = new GameObject("Shore breaker sound"); child.transform.SetParent(transform, false);
            target.Source = child.AddComponent<AudioSource>();
            target.Source.playOnAwake = false; target.Source.spatialBlend = 1f;
            target.Source.dopplerLevel = 0f; target.Source.minDistance = 8f;
            target.Source.maxDistance = searchRadius + 20f; target.Source.rolloffMode = AudioRolloffMode.Linear;
        }
        target.Source.Stop(); target.Source.clip = waveClips[clip];
        Vector2 window = playbackWindows != null && clip < playbackWindows.Length ? playbackWindows[clip] : default;
        float start = Mathf.Clamp(window.x, 0f, Mathf.Max(0f, target.Source.clip.length - .01f));
        target.End = window.y > start ? Mathf.Min(window.y, target.Source.clip.length) : target.Source.clip.length;
        target.Position = sample.Position; target.Started = Time.time;
        target.Source.transform.position = target.Position; target.Source.time = start;
        target.Source.volume = volume;
        target.Source.Play(); lastClip = clip;
    }

    private void UpdateVoices(float deltaTime)
    {
        float blend = 1f - Mathf.Exp(-Mathf.Max(0f, deltaTime) / .2f);
        foreach (var voice in voices)
        {
            if (voice.Source == null) continue;
            // These are shore locations, not children that travel with the player.
            voice.Source.transform.position = voice.Position;
            if (voice.Source.isPlaying && voice.Source.time >= voice.End) voice.Source.Stop();
            if (!voice.Source.isPlaying) continue;
            float targetVolume = 0f;
            if (waves != null && probes != null)
                for (int i = 0; i < probes.Length; i++)
                    if (probes[i].Active && waves.TryGetSample(i, out var sample)
                        && HorizontalSquared(sample.Position, voice.Position) < .25f)
                    { targetVolume = WaveVolume(sample); break; }
            voice.Source.volume = Mathf.Lerp(voice.Source.volume, targetVolume, blend);
        }
    }

    private float WaveVolume(OceanShoreSampler.Sample sample)
        // Amplitude already includes shelter. Cap after saturation as well:
        // a storm breaker must not stay at full volume in a one-third-size lee.
        => maximumVolume * Mathf.Min(sample.WaveScale,
            Mathf.Clamp01(sample.Amplitude / Mathf.Max(.01f, fullVolumeWaveAmplitude)));
    private void StopVoices() { foreach (var voice in voices) if (voice.Source != null) voice.Source.Stop(); }
    private void ReleaseQueries() { placement?.Dispose(); waves?.Dispose(); placement = waves = null; }
    private void OnDisable() { ReleaseQueries(); StopVoices(); }
    private static float HorizontalSquared(Vector3 a, Vector3 b) => (new Vector2(a.x - b.x, a.z - b.z)).sqrMagnitude;

    private void OnDrawGizmosSelected()
    {
        if (probes == null) return;
        Gizmos.color = Color.cyan;
        foreach (var probe in probes) if (probe.Active) Gizmos.DrawWireSphere(probe.Position, .5f);
    }
}
