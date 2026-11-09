using System;
using UnityEngine;

namespace Motu.World
{
    /// <summary>A circular local wind gain, independent of the ambient wind that advects it.</summary>
    public readonly struct WorldSquallField
    {
        public readonly Vector2 Center;
        public readonly float RadiusMetres, EdgeFraction, WindMultiplier, Strength;
        public readonly Vector2 Velocity;

        public WorldSquallField(Vector2 center, float radiusMetres, float edgeFraction,
            float windMultiplier, float strength, Vector2 velocity)
        {
            if (!Finite(center) || !Finite(velocity) || !float.IsFinite(radiusMetres * radiusMetres)
                || !float.IsFinite(edgeFraction) || !float.IsFinite(windMultiplier) || !float.IsFinite(strength))
                throw new ArgumentException("Squall field parameters must be finite.");
            Center = center;
            RadiusMetres = Mathf.Max(0f, radiusMetres);
            EdgeFraction = Mathf.Clamp01(edgeFraction);
            WindMultiplier = Mathf.Max(1f, windMultiplier);
            Strength = Mathf.Clamp01(strength);
            Velocity = velocity;
        }

        /// <summary>Spatial coverage before the lifecycle strength is applied.</summary>
        public float WeightAt(Vector2 worldPosition)
        {
            float outerSquared = RadiusMetres * RadiusMetres;
            float distanceSquared = (worldPosition - Center).sqrMagnitude;
            if (outerSquared <= 0f || distanceSquared >= outerSquared) return 0f;
            float inner = RadiusMetres * (1f - EdgeFraction);
            float innerSquared = inner * inner;
            if (distanceSquared <= innerSquared) return 1f;
            float t = Mathf.Clamp01((distanceSquared - innerSquared) / (outerSquared - innerSquared));
            return 1f - t * t * (3f - 2f * t);
        }

        public float MultiplierAt(Vector2 worldPosition) =>
            1f + (WindMultiplier - 1f) * Strength * WeightAt(worldPosition);

        // Shared with WorldSquallField.cginc. Pack true world X/Z positions;
        // mesh recentering and the amplified local wind never move the field.
        public Vector4 ShaderBounds
        {
            get
            {
                float inner = RadiusMetres * (1f - EdgeFraction);
                return new Vector4(Center.x, Center.y, inner * inner, RadiusMetres * RadiusMetres);
            }
        }
        public Vector4 ShaderWind => new Vector4((WindMultiplier - 1f) * Strength, Velocity.x, Velocity.y, 0f);

        private static bool Finite(Vector2 value) => float.IsFinite(value.x) && float.IsFinite(value.y);
    }
}
