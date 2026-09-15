using System.Collections.Generic;
using System.Diagnostics;
using Motu.Rendering;
using UnityEngine;

namespace Motu.Editor
{
    public static class NoiseBenchmark
    {
        public static void Run()
        {
            Measure(false); Measure(true); // warm both paths
            for (var run = 0; run < 3; run++)
                UnityEngine.Debug.Log($"MOTU NOISE run={run + 1} four resident islands: original={Measure(false):F2} ms shared={Measure(true):F2} ms");
        }
        private static double Measure(bool shared)
        {
            var textures = new List<Texture>();
            var leases = new List<SharedNoiseTexture>();
            var watch = Stopwatch.StartNew();
            for (var i = 0; i < 4; i++)
                if (shared)
                    foreach (var kind in new[] { SharedNoiseTexture.Kind.Cliff, SharedNoiseTexture.Kind.River, SharedNoiseTexture.Kind.Weather })
                        leases.Add(SharedNoiseTexture.Acquire(kind));
                else
                {
                    textures.Add(ProceduralNoiseTextures.CreateCliffNoiseTexture());
                    textures.Add(ProceduralNoiseTextures.CreateRiverNoiseTexture());
                    textures.Add(ProceduralNoiseTextures.CreateWeatherNoiseTexture());
                }
            watch.Stop();
            foreach (var texture in textures) Object.DestroyImmediate(texture);
            foreach (var lease in leases) lease.Dispose();
            return watch.Elapsed.TotalMilliseconds;
        }
    }
}
