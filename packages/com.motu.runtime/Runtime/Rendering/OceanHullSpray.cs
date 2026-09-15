using System;
using Motu.World;
using UnityEngine;
using UnityEngine.Rendering;

namespace Motu.Rendering
{
    [DisallowMultipleComponent]
    public sealed class OceanHullSpray : MonoBehaviour
    {
        [SerializeField] private OceanSurfaceController ocean;
        [SerializeField] private Material sprayMaterial;
        [Tooltip("Closed waterline in ship-local coordinates, ordered around the authored waterline normal. Do not repeat the first point.")]
        [SerializeField] private Vector3[] waterline = Array.Empty<Vector3>();
        [SerializeField, HideInInspector] private Vector3 waterlineUp = Vector3.up;
        [Min(.1f)] [SerializeField] private float sampleSpacing = 1f;
        [Range(8, OceanSurfaceSampler.MaximumCount)] [SerializeField] private int maximumSamples = 256;
        [Min(0)] [SerializeField] private float velocityPerMetre = 6f;
        [Min(0)] [SerializeField] private float maximumSpeed = 18f;
        [Range(0, 90)] [SerializeField] private float launchAngle = 65f;
        [Range(0, 30)] [SerializeField] private float directionSpread = 12f;
        [Tooltip("Particles per second, per metre of waterline, per metre of wave penetration.")]
        [Min(0)] [SerializeField] private float emissionDensity = 24f;
        [Min(1)] [SerializeField] private int maximumEmissionRate = 1500;
        [Min(1)] [SerializeField] private int maximumParticles = 2500;
        [Min(.01f)] [SerializeField] private float lifetime = .9f;
        [Min(.001f)] [SerializeField] private float particleSize = .12f;
        [Tooltip("Additional spray strength per m/s of water closing on the hull. Zero uses only the absolute world-space height gap.")]
        [Min(0)] [SerializeField] private float relativeVelocityInfluence = .25f;
        [Min(.01f)] [SerializeField] private float maximumSampleTravel = 1f;
        [Min(.01f)] [SerializeField] private float maximumSampleAge = .2f;

        private OceanSurfaceSampler sampler;
        private Rigidbody body;
        private ParticleSystem particles;
        private ParticleSystemRenderer particleRenderer;
        private Vector3[] worldPoints, waveVelocities;
        private float[] depths, strengthMultipliers, fractions;
        private bool[] valid;
        private WetSegment[] segments;
        private float nextOceanSearch;
        private float emissionBudget;
        private int firstSegment;

        public float SampleSpacing => sampleSpacing;
        public int MaximumSamples => maximumSamples;
        public int SampleCount => waterline.Length;
        internal ParticleSystem Particles => particles;

        public void SetWaterline(Vector3[] points, Material material)
        {
            waterline = (Vector3[])points.Clone();
            sprayMaterial = material;
            waterlineUp = transform.InverseTransformDirection(Vector3.up);
            ReleaseSampler();
        }

        private void OnEnable()
        {
            body = GetComponentInParent<Rigidbody>();
            if (particles != null) particles.Play();
        }

        private void LateUpdate()
        {
            if (ocean == null && Time.unscaledTime >= nextOceanSearch)
            {
                ocean = FindFirstObjectByType<OceanSurfaceController>();
                nextOceanSearch = Time.unscaledTime + 1f;
            }
            if (waterline == null || waterline.Length < 3 || waterline.Length > OceanSurfaceSampler.MaximumCount || ocean == null
                || !ocean.isActiveAndEnabled || ocean.SurfaceMaterial == null || sprayMaterial == null)
            { ResetEmission(); return; }
            if (sampler == null || sampler.Count != waterline.Length)
            {
                ReleaseSampler();
                try { sampler = new OceanSurfaceSampler(waterline.Length, includeVelocity: true); }
                catch (NotSupportedException error) { Debug.LogError(error.Message, this); enabled = false; return; }
                worldPoints = new Vector3[waterline.Length];
                depths = new float[waterline.Length];
                waveVelocities = new Vector3[waterline.Length];
                strengthMultipliers = new float[waterline.Length];
                fractions = new float[waterline.Length];
                valid = new bool[waterline.Length];
                segments = new WetSegment[waterline.Length];
            }
            if (particles == null) CreateParticles();
            var main = particles.main;
            if (main.maxParticles != maximumParticles)
            {
                // Unity limits future emission but does not trim live particles
                // when the inspector budget is lowered. Retain only the new cap.
                if (particles.particleCount > maximumParticles)
                {
                    var retained = new ParticleSystem.Particle[maximumParticles];
                    var count = particles.GetParticles(retained);
                    particles.SetParticles(retained, count);
                }
                main.maxParticles = maximumParticles;
            }
            particleRenderer.sharedMaterial = sprayMaterial;
            var dt = Mathf.Min(Time.deltaTime, .05f);
            for (var i = 0; i < waterline.Length; i++)
            {
                var point = transform.TransformPoint(waterline[i]);
                worldPoints[i] = point;
                valid[i] = sampler.TryGetSurface(i, point, maximumSampleTravel, out var height, out waveVelocities[i])
                    && sampler.SampleAge(i) <= maximumSampleAge;
                depths[i] = valid[i] ? height - point.y : 0;
            }
            EmitSegments(dt);
            sampler.Submit(ocean, worldPoints);
        }

        private void EmitSegments(float dt)
        {
            var demand = 0f;
            for (var i = 0; i < worldPoints.Length; i++)
            {
                var j = (i + 1) % worldPoints.Length;
                segments[i] = valid[i] && valid[j]
                    ? WetSegment.Between(depths[i], depths[j], Vector3.Distance(worldPoints[i], worldPoints[j])) : default;
                var tangent = (worldPoints[j] - worldPoints[i]).normalized;
                var outward = Vector3.Cross(transform.TransformDirection(waterlineUp), tangent).normalized;
                var shipVelocity = body != null ? body.GetPointVelocity((worldPoints[i] + worldPoints[j]) * .5f) : Vector3.zero;
                var waveVelocity = (waveVelocities[i] + waveVelocities[j]) * .5f;
                strengthMultipliers[i] = StrengthMultiplier(shipVelocity, waveVelocity, outward, relativeVelocityInfluence);
                demand += segments[i].Integral * strengthMultipliers[i];
                if (segments[i].Integral <= 0) fractions[i] = 0;
            }
            // Keep at most one short frame's credit, so neither GPU delays nor
            // particle limits accumulate a burst. The budget also bounds rounding.
            emissionBudget = Mathf.Min(maximumEmissionRate * .05f + 1, emissionBudget + maximumEmissionRate * dt);
            var available = Mathf.Min(Mathf.FloorToInt(emissionBudget), Mathf.Max(0, maximumParticles - particles.particleCount));
            var expected = demand * emissionDensity * dt;
            var scale = expected > 0 ? Mathf.Min(1, Mathf.Min(maximumEmissionRate * dt, available) / expected) : 0;
            for (var offset = 0; offset < worldPoints.Length; offset++)
            {
                var i = (firstSegment + offset) % worldPoints.Length;
                var segment = segments[i];
                if (segment.Integral <= 0) continue;
                var total = fractions[i] + segment.Integral * strengthMultipliers[i] * emissionDensity * dt * scale;
                var count = Mathf.FloorToInt(total);
                fractions[i] = total - count; // Discard blocked whole particles, never catch up later.
                count = Mathf.Min(count, available);
                available -= count;
                emissionBudget -= count;
                var j = (i + 1) % worldPoints.Length;
                var tangent = (worldPoints[j] - worldPoints[i]).normalized;
                var outward = Vector3.Cross(transform.TransformDirection(waterlineUp), tangent).normalized;
                for (var p = 0; p < count; p++)
                {
                    var t = segment.Sample(UnityEngine.Random.value);
                    var position = Vector3.Lerp(worldPoints[i], worldPoints[j], t);
                    var depth = Mathf.Max(0, Mathf.Lerp(depths[i], depths[j], t));
                    var angle = (launchAngle + UnityEngine.Random.Range(-directionSpread, directionSpread)) * Mathf.Deg2Rad;
                    var direction = (outward * Mathf.Cos(angle) + Vector3.up * Mathf.Sin(angle)).normalized;
                    direction = Quaternion.AngleAxis(UnityEngine.Random.Range(-directionSpread, directionSpread), Vector3.up) * direction;
                    var velocity = direction * LaunchSpeed(depth * strengthMultipliers[i], velocityPerMetre, maximumSpeed);
                    if (body != null) velocity += body.GetPointVelocity(position);
                    particles.Emit(new ParticleSystem.EmitParams
                    {
                        position = position,
                        velocity = velocity,
                        startLifetime = lifetime * UnityEngine.Random.Range(.7f, 1.3f),
                        startSize = particleSize * UnityEngine.Random.Range(.65f, 1.4f),
                        startColor = new Color(.86f, .93f, .96f, .75f)
                    }, 1);
                }
            }
            firstSegment = (firstSegment + 1) % worldPoints.Length;
        }

        internal static float StrengthMultiplier(Vector3 shipVelocity, Vector3 waveVelocity, Vector3 outward, float influence)
        {
            var relative = shipVelocity - waveVelocity;
            var horizontalOutward = Vector3.ProjectOnPlane(outward, Vector3.up).normalized;
            var closingSpeed = Mathf.Max(0, Vector3.Dot(relative, horizontalOutward)) + Mathf.Max(0, -relative.y);
            return 1 + Mathf.Max(0, influence) * closingSpeed;
        }

        internal static float LaunchSpeed(float depth, float gain, float cap)
            => Mathf.Min(Mathf.Max(0, cap), Mathf.Max(0, depth) * Mathf.Max(0, gain));

        internal readonly struct WetSegment
        {
            internal readonly float Start, End, DepthStart, DepthEnd, Integral;
            private WetSegment(float start, float end, float a, float b, float length)
            { Start = start; End = end; DepthStart = a; DepthEnd = b; Integral = length * (end - start) * (a + b) * .5f; }

            internal static WetSegment Between(float a, float b, float length)
            {
                if (a <= 0 && b <= 0) return default;
                var start = a < 0 ? a / (a - b) : 0;
                var end = b < 0 ? a / (a - b) : 1;
                return new WetSegment(start, end, Mathf.Max(0, a), Mathf.Max(0, b), length);
            }

            internal float Sample(float uniform)
            {
                // Invert the integral of the linear density across the wet interval.
                var area = Mathf.Clamp01(uniform) * (DepthStart + DepthEnd) * .5f;
                var denominator = DepthStart + Mathf.Sqrt(Mathf.Max(0,
                    DepthStart * DepthStart + 2 * (DepthEnd - DepthStart) * area));
                var t = uniform >= 1 ? 1 : denominator > .000001f ? 2 * area / denominator : 0;
                return Mathf.Lerp(Start, End, t);
            }
        }

        private void CreateParticles()
        {
            var child = new GameObject("Waterline spray") { hideFlags = HideFlags.DontSave, layer = gameObject.layer };
            child.transform.SetParent(transform, false);
            particles = child.AddComponent<ParticleSystem>();
            particles.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
            var main = particles.main;
            main.playOnAwake = false;
            main.simulationSpace = ParticleSystemSimulationSpace.World;
            main.scalingMode = ParticleSystemScalingMode.Shape;
            main.maxParticles = maximumParticles;
            main.gravityModifier = 1f;
            main.cullingMode = ParticleSystemCullingMode.AlwaysSimulate;
            var emission = particles.emission;
            emission.enabled = false;
            var shape = particles.shape;
            shape.enabled = false;
            var collision = particles.collision;
            collision.enabled = false; // Spray is deliberately allowed to leave through the hull.
            var colour = particles.colorOverLifetime;
            colour.enabled = true;
            var gradient = new Gradient();
            gradient.SetKeys(new[] { new GradientColorKey(Color.white, 0), new GradientColorKey(Color.white, 1) },
                new[] { new GradientAlphaKey(1, 0), new GradientAlphaKey(.8f, .35f), new GradientAlphaKey(0, 1) });
            colour.color = gradient;
            var size = particles.sizeOverLifetime;
            size.enabled = true;
            size.size = new ParticleSystem.MinMaxCurve(1, AnimationCurve.Linear(0, 1, 1, .2f));
            var renderer = particles.GetComponent<ParticleSystemRenderer>();
            particleRenderer = renderer;
            renderer.sharedMaterial = sprayMaterial;
            renderer.renderMode = ParticleSystemRenderMode.Stretch;
            renderer.velocityScale = .06f;
            renderer.lengthScale = .4f;
            renderer.cameraVelocityScale = 0;
            renderer.shadowCastingMode = ShadowCastingMode.Off;
            renderer.receiveShadows = true;
            renderer.lightProbeUsage = LightProbeUsage.Off;
            renderer.reflectionProbeUsage = ReflectionProbeUsage.Off;
            particles.Play();
        }

        private void ResetEmission()
        {
            emissionBudget = 0;
            if (fractions == null) return;
            Array.Clear(fractions, 0, fractions.Length);
        }

        private void ReleaseSampler() { sampler?.Dispose(); sampler = null; ResetEmission(); }
        private void OnDisable() { ReleaseSampler(); if (particles != null) particles.Stop(true, ParticleSystemStopBehavior.StopEmitting); }
        private void OnDestroy() { if (particles != null) UnityObjectLifetime.DestroyUnityObject(particles.gameObject); }

        private void OnDrawGizmosSelected()
        {
            if (waterline == null || waterline.Length < 3) return;
            Gizmos.color = Color.cyan;
            for (var i = 0; i < waterline.Length; i++)
            {
                var a = transform.TransformPoint(waterline[i]);
                var b = transform.TransformPoint(waterline[(i + 1) % waterline.Length]);
                Gizmos.DrawLine(a, b);
                Gizmos.DrawSphere(a, .06f);
                Gizmos.DrawRay((a + b) * .5f, Vector3.Cross(transform.TransformDirection(waterlineUp), (b - a).normalized) * .4f);
            }
        }
    }
}
