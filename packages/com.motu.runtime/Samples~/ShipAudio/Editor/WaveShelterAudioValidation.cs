using System;
using System.IO;
using System.Reflection;
using System.Text;
using Motu.World;
using UnityEditor;
using UnityEngine;
using Object = UnityEngine.Object;

// Uses real AudioSources in the existing editor without changing the open scene.
public static class WaveShelterAudioValidation
{
    private const BindingFlags Hidden = BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public;
    public const string ReportPath = "/tmp/terranullus-wave-shelter-audio.txt";

    [MenuItem("Island/Validate Sheltered Wave Audio")]
    public static void Run()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode)
            throw new InvalidOperationException("Stop Play mode before running the isolated audio validation.");
        var report = new StringBuilder();
        var root = new GameObject("Temporary sheltered audio validation") { hideFlags = HideFlags.HideAndDontSave };
        var clip = AudioClip.Create("Silent validation wave", 44100 * 4, 1, 44100, false);
        try
        {
            var hull = root.AddComponent<HullWaveAudio>();
            Set(hull, "waveClips", new[] { clip });
            foreach (var speed in new[] { .25f, 2f, 20f })
            {
                float openVolume = 0f;
                foreach (var scale in new[] { 1f, 1f / 3f })
                {
                    Call(hull, "StopVoices");
                    Call(hull, "PlayImpact", 0, Vector3.zero, speed, scale);
                    var voice = ((Array)Get(hull, "voices")).GetValue(0);
                    var source = (AudioSource)Get(voice, "Source");
                    if (scale == 1f) openVolume = source.volume;
                    else Require(Mathf.Abs(source.volume - openVolume / 3f) < .0001f,
                        "Hull impact retains exactly one-third gain at speed " + speed, report);
                    // The minimum audible attack must not restore exposed-water gain.
                    Set(voice, "MinimumEndTime", Time.time + 2f);
                    Call(hull, "FadeVoice", voice, .1f);
                    Require(source.volume <= openVolume * scale + .0001f,
                        "Contact hold preserves sheltered gain", report);
                }
            }

            Call(hull, "StopVoices");
            Call(hull, "PlayImpact", 0, Vector3.zero, 20f, 0f);
            var hullVoice = ((Array)Get(hull, "voices")).GetValue(0);
            Require(!((AudioSource)Get(hullVoice, "Source")).isPlaying,
                "Flat lee does not start a hull wave clip", report);

            var sampleType = typeof(HullWaveAudio).GetNestedType("SurfaceSample", BindingFlags.NonPublic);
            var samples = Array.CreateInstance(sampleType, 4);
            var points = new[] { new Vector3(-1, 0, 0), new Vector3(1, 0, 0),
                new Vector3(1, 0, 4), new Vector3(-1, 0, 4) };
            for (int i = 0; i < 4; i++)
            {
                var sample = Activator.CreateInstance(sampleType);
                Set(sample, "Position", points[i]); Set(sample, "Valid", true);
                Set(sample, "Depth", i == 0 || i == 3 ? 0f : .24f);
                Set(sample, "WaveScale", i == 0 || i == 3 ? 0f : 2f / 3f);
                samples.SetValue(sample, i);
            }
            Set(hull, "samples", samples);
            Set(hullVoice, "Coordinate", .5f);
            Set(hullVoice, "LocalContact", new Vector3(0, .12f, 0));
            object[] follow = { hullVoice, .1f, 0f, Vector3.zero, 0f };
            Require((bool)Call(hull, "TryFollowFront", follow)
                && Mathf.Abs((float)follow[4] - 1f / 3f) < .0001f,
                "Moving hull contact interpolates shelter between its wave probes", report);

            Call(hull, "PlayImpact", 0, new Vector3(0, .12f, 0), 2f, 1f);
            Call(hull, "UpdateVoices", 1f);
            hullVoice = ((Array)Get(hull, "voices")).GetValue(0);
            var hullSource = (AudioSource)Get(hullVoice, "Source");
            Require(hullSource.isPlaying && Mathf.Abs(hullSource.volume - .7f / 3f) < .001f,
                "Already playing hull clip quietens as its contact enters shelter", report);
            for (int i = 0; i < samples.Length; i++)
            {
                var sample = samples.GetValue(i); Set(sample, "WaveScale", 1f); samples.SetValue(sample, i);
            }
            Call(hull, "UpdateVoices", 1f);
            Require(Mathf.Abs(hullSource.volume - .7f) < .001f,
                "Already playing hull clip recovers as its contact leaves shelter", report);
            Call(hull, "StopVoices");

            var shore = root.AddComponent<ShoreWaveAudio>();
            Set(shore, "waveClips", new[] { clip });
            foreach (var amplitude in new[] { .1f, 5f })
            {
                float open = (float)Call(shore, "WaveVolume", ShoreSample(amplitude, 1f));
                var sheltered = ShoreSample(amplitude / 3f, 1f / 3f);
                float lee = (float)Call(shore, "WaveVolume", sheltered);
                Require(Mathf.Abs(lee - open / 3f) < .0001f,
                    "Shore gain follows one-third wave height even above volume saturation: " + amplitude + " m", report);
                Call(shore, "PlayWave", sheltered);
                var voice = ((Array)Get(shore, "voices")).GetValue(0);
                Require(Mathf.Abs(((AudioSource)Get(voice, "Source")).volume - lee) < .0001f,
                    "Sheltered gain reaches the shore AudioSource", report);
                Call(shore, "StopVoices");
            }

            using (var query = new OceanShoreSampler(1))
            {
                Set(shore, "waves", query);
                var probeType = typeof(ShoreWaveAudio).GetNestedType("Probe", BindingFlags.NonPublic);
                var probes = Array.CreateInstance(probeType, 1);
                var probe = Activator.CreateInstance(probeType); Set(probe, "Active", true);
                probes.SetValue(probe, 0); Set(shore, "probes", probes);
                Set(query, "<Revision>k__BackingField", 1);
                var cache = (OceanShoreSampler.Sample[])Get(query, "samples");
                Call(shore, "PlayWave", ShoreSample(5f, 1f));
                var voice = ((Array)Get(shore, "voices")).GetValue(0);
                var source = (AudioSource)Get(voice, "Source");
                Require(source.isPlaying, "Native shore clip is playing for live-volume checks", report);
                cache[0] = ShoreSample(5f / 3f, 1f / 3f);
                Call(shore, "UpdateVoices", 2f);
                Require(Mathf.Abs(source.volume - .65f / 3f) < .001f,
                    "Already playing shore clip quietens when its patch becomes sheltered", report);
                cache[0] = ShoreSample(5f, 1f);
                Call(shore, "UpdateVoices", 2f);
                Require(Mathf.Abs(source.volume - .65f) < .001f,
                    "Already playing shore clip recovers in exposed water", report);
                Set(query, "<Revision>k__BackingField", 0);
                Call(shore, "UpdateVoices", 2f);
                Require(source.volume < .001f, "Missing wave samples fade shore clips to silence", report);
            }
            report.AppendLine("SUCCESS: Native sheltered wave audio validation passed.");
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); Debug.LogException(error); }
        finally
        {
            Object.DestroyImmediate(root); Object.DestroyImmediate(clip);
            File.WriteAllText(ReportPath, report.ToString());
            Debug.Log(report.ToString());
        }
    }

    private static OceanShoreSampler.Sample ShoreSample(float amplitude, float scale)
        => (OceanShoreSampler.Sample)typeof(OceanShoreSampler.Sample).GetConstructors(Hidden)[0].Invoke(
            new object[] { Vector3.zero, 4f, 1f, 1f, Time.time, amplitude, scale });
    private static object Get(object target, string name) => target.GetType().GetField(name, Hidden).GetValue(target);
    private static void Set(object target, string name, object value) => target.GetType().GetField(name, Hidden).SetValue(target, value);
    private static object Call(object target, string name, params object[] arguments)
        => target.GetType().GetMethod(name, Hidden).Invoke(target, arguments);
    private static void Require(bool condition, string message, StringBuilder report)
    {
        if (!condition) throw new InvalidOperationException(message);
        report.AppendLine("PASS: " + message);
    }
}
