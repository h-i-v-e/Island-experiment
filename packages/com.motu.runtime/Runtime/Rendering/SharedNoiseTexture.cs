using System;
using UnityEngine;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Rendering
{
    /// <summary>Main-thread leases for identical, immutable procedural noise used by resident islands.</summary>
    internal sealed class SharedNoiseTexture : IDisposable
    {
        internal enum Kind { Cliff, River, Weather }
        private sealed class Entry { internal Texture texture; internal int users; }
        private static readonly Entry[] Entries = { new Entry(), new Entry(), new Entry() };
        private Entry entry;
        internal Texture Texture => entry?.texture;

        private SharedNoiseTexture(Entry entry) { this.entry = entry; }

        internal static SharedNoiseTexture Acquire(Kind kind)
        {
            var entry = Entries[(int)kind];
            if (entry.texture == null)
                entry.texture = kind switch {
                    Kind.Cliff => ProceduralNoiseTextures.CreateCliffNoiseTexture(),
                    Kind.River => ProceduralNoiseTextures.CreateRiverNoiseTexture(),
                    Kind.Weather => ProceduralNoiseTextures.CreateWeatherNoiseTexture(),
                    _ => throw new ArgumentOutOfRangeException(nameof(kind))
                };
            entry.users++;
            return new SharedNoiseTexture(entry);
        }

        public void Dispose()
        {
            if (entry == null) return;
            var owned = entry;
            entry = null;
            if (--owned.users != 0) return;
            DestroyUnityObject(owned.texture);
            owned.texture = null;
        }
    }
}
