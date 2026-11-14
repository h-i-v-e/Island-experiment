using System;
using UnityEngine;

namespace Motu.World
{
    /// <summary>A shared asynchronous batch using the same waves and weather as the rendered sea.</summary>
    public sealed class OceanSurfaceProbeBatch : IDisposable
    {
        private readonly OceanSurfaceSampler sampler;
        public static bool Supported => OceanSurfaceSampler.Supported;

        public OceanSurfaceProbeBatch(int count) => sampler = new OceanSurfaceSampler(count);
        public void Submit(OceanSurfaceController ocean, Vector3[] points) => sampler.Submit(ocean, points);
        public bool TryHeight(int index, Vector3 position, float maximumTravel, out float height, out float age)
            => TryHeight(index, position, maximumTravel, out height, out age, out _);

        public bool TryHeight(int index, Vector3 position, float maximumTravel, out float height, out float age, out float travel)
        {
            age = sampler.SampleAge(index);
            travel = Vector3.ProjectOnPlane(position - sampler.SamplePosition(index), Vector3.up).magnitude;
            return sampler.TryGetHeight(index, position, maximumTravel, out height, out _, .1f, .25f);
        }

        public void Dispose() => sampler.Dispose();
    }
}
