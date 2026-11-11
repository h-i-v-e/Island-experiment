using System;
using UnityEngine;

namespace Motu.Gameplay
{
    /// <summary>Approximate cruising performance and route-planning limits for a virtual ship.</summary>
    [Serializable]
    public struct SailingNavigationSettings
    {
        [Tooltip("Angle from the wind source inside which the virtual ship cannot make sail-driven progress."), Range(0f, 89f)]
        public float MinimumWindAngleDegrees;
        [Tooltip("Extra clearance outside the no-sail angle when choosing a tack."), Range(1f, 89f)]
        public float TackMarginDegrees;
        [Tooltip("Maximum distance from the voyage line before changing tack; narrows near the destination."), Min(1f)]
        public float TackWidthMetres;
        [Tooltip("Minimum time on a tack before switching sides."), Min(0f)]
        public float MinimumTackSeconds;
        [Tooltip("Horizontal distance from the destination at which the voyage is complete."), Min(0f)]
        public float ArrivalRadiusMetres;
        [Tooltip("Maximum virtual heading change in degrees per second."), Min(0f)]
        public float VirtualTurnRateDegrees;
        [Tooltip("Maximum change in virtual velocity per second."), Min(0f)]
        public float AccelerationMetresPerSecondSquared;
        [Tooltip("Approximate beam-reach boat speed as a fraction of true wind speed, before the forward multiplier."), Min(0f)]
        public float WindSpeedRatio;
        [Tooltip("Travel multiplier matching the Explorer's forward speed setting."), Min(0f)]
        public float ForwardSpeedMultiplier;

        public static SailingNavigationSettings Default => new SailingNavigationSettings
        {
            MinimumWindAngleDegrees = 45f,
            TackMarginDegrees = 15f,
            TackWidthMetres = 150f,
            MinimumTackSeconds = 20f,
            ArrivalRadiusMetres = 25f,
            VirtualTurnRateDegrees = 4f,
            AccelerationMetresPerSecondSquared = .25f,
            WindSpeedRatio = .25f,
            ForwardSpeedMultiplier = 3f
        };

        /// <summary>Clamp invalid values. An entirely zero-initialized value uses the standard defaults.</summary>
        public SailingNavigationSettings Validated()
        {
            if (MinimumWindAngleDegrees == 0f && TackMarginDegrees == 0f && TackWidthMetres == 0f
                && MinimumTackSeconds == 0f && ArrivalRadiusMetres == 0f && VirtualTurnRateDegrees == 0f
                && AccelerationMetresPerSecondSquared == 0f && WindSpeedRatio == 0f && ForwardSpeedMultiplier == 0f)
                return Default;
            var fallback = Default;
            var result = this;
            result.MinimumWindAngleDegrees = Mathf.Clamp(FiniteOr(MinimumWindAngleDegrees, fallback.MinimumWindAngleDegrees), 0f, 89f);
            result.TackMarginDegrees = Mathf.Clamp(FiniteOr(TackMarginDegrees, fallback.TackMarginDegrees), 1f, 89f);
            result.TackWidthMetres = Mathf.Max(1f, FiniteOr(TackWidthMetres, fallback.TackWidthMetres));
            result.MinimumTackSeconds = Mathf.Max(0f, FiniteOr(MinimumTackSeconds, fallback.MinimumTackSeconds));
            result.ArrivalRadiusMetres = Mathf.Max(0f, FiniteOr(ArrivalRadiusMetres, fallback.ArrivalRadiusMetres));
            result.VirtualTurnRateDegrees = Mathf.Max(0f, FiniteOr(VirtualTurnRateDegrees, fallback.VirtualTurnRateDegrees));
            result.AccelerationMetresPerSecondSquared = Mathf.Max(0f, FiniteOr(AccelerationMetresPerSecondSquared, fallback.AccelerationMetresPerSecondSquared));
            result.WindSpeedRatio = Mathf.Max(0f, FiniteOr(WindSpeedRatio, fallback.WindSpeedRatio));
            result.ForwardSpeedMultiplier = Mathf.Max(0f, FiniteOr(ForwardSpeedMultiplier, fallback.ForwardSpeedMultiplier));
            return result;
        }

        private static float FiniteOr(float value, float fallback) =>
            float.IsNaN(value) || float.IsInfinity(value) ? fallback : value;
    }

    /// <summary>Serializable navigation state; positions and velocities use world X/Z coordinates.</summary>
    [Serializable]
    public struct SailingNavigationState
    {
        public Vector2 Position;
        public Vector2 Velocity;
        public float HeadingDegrees;
        public int TackSide;
        public float SecondsOnTack;
        public bool Arrived;

        // Persist the corridor across ticks and mode changes; reset it when the destination changes.
        public Vector2 RouteOrigin;
        public Vector2 RouteDestination;
        public bool HasRoute;

        public SailingNavigationState(Vector2 position, float heading)
        {
            Position = position;
            Velocity = Vector2.zero;
            HeadingDegrees = Mathf.Repeat(heading, 360f);
            TackSide = 0;
            SecondsOnTack = 0f;
            Arrived = false;
            RouteOrigin = position;
            RouteDestination = position;
            HasRoute = false;
        }
    }

    /// <summary>
    /// Shared route decisions and a lightweight virtual approximation. The polar is not a
    /// physical sail model; it estimates speed from true wind and heading only.
    /// Wind velocity points in the direction the air travels, in world X/Z coordinates.
    /// </summary>
    public static class SailingNavigation
    {
        public static float DesiredHeading(ref SailingNavigationState state, Vector2 destination,
            Vector2 windVelocity, float deltaTime, SailingNavigationSettings settings)
        {
            settings = settings.Validated();
            if (!state.HasRoute || state.RouteDestination != destination)
            {
                state.HasRoute = true;
                state.RouteOrigin = state.Position;
                state.RouteDestination = destination;
                state.TackSide = 0;
                state.SecondsOnTack = 0f;
                state.Arrived = false;
            }

            // Arrival belongs to the voyage, not the current anchor swing or drift.
            if (state.Arrived) return state.HeadingDegrees;

            Vector2 toDestination = destination - state.Position;
            if (toDestination.magnitude <= Mathf.Max(0f, settings.ArrivalRadiusMetres))
            {
                state.Arrived = true;
                return state.HeadingDegrees;
            }
            if (windVelocity.sqrMagnitude <= .01f) return state.HeadingDegrees;

            Vector2 upwind = -windVelocity.normalized;
            float directAngle = Vector2.Angle(upwind, toDestination);
            float tackAngle = TackAngle(settings);
            if (directAngle >= tackAngle)
            {
                state.TackSide = 0;
                state.SecondsOnTack = 0f;
                return Bearing(toDestination);
            }

            Vector2 route = destination - state.RouteOrigin;
            if (route.sqrMagnitude < .0001f) route = toDestination;
            route.Normalize();
            Vector2 across = new Vector2(route.y, -route.x);
            float offset = Vector2.Dot(state.Position - state.RouteOrigin, across);
            Vector2 positive = Rotate(upwind, tackAngle);
            float positiveAcross = Vector2.Dot(positive, across);
            if (state.TackSide == 0)
            {
                // Steer towards the corridor centre, or choose a deterministic first tack.
                state.TackSide = offset * positiveAcross > 0f ? -1 : 1;
                state.SecondsOnTack = 0f;
            }
            else
            {
                state.SecondsOnTack += Mathf.Max(0f, deltaTime);
                float width = Mathf.Min(Mathf.Max(1f, settings.TackWidthMetres),
                    Mathf.Max(1f, toDestination.magnitude * .25f));
                float tackAcross = Vector2.Dot(Rotate(upwind, tackAngle * state.TackSide), across);
                if (state.SecondsOnTack >= Mathf.Max(0f, settings.MinimumTackSeconds)
                    && Mathf.Abs(offset) >= width && offset * tackAcross > 0f)
                {
                    state.TackSide = -state.TackSide;
                    state.SecondsOnTack = 0f;
                }
            }
            return Bearing(Rotate(upwind, tackAngle * state.TackSide));
        }

        public static void AdvanceVirtual(ref SailingNavigationState state, float desiredHeading,
            Vector2 windVelocity, float deltaTime, SailingNavigationSettings settings)
        {
            settings = settings.Validated();
            float dt = Mathf.Max(0f, deltaTime);
            if (dt == 0f || state.Arrived) return;
            state.HeadingDegrees = Mathf.Repeat(Mathf.MoveTowardsAngle(state.HeadingDegrees, desiredHeading,
                Mathf.Max(0f, settings.VirtualTurnRateDegrees) * dt), 360f);
            float speed = SpeedAtHeading(state.HeadingDegrees, windVelocity, settings);
            Vector2 targetVelocity = Direction(state.HeadingDegrees) * speed;
            state.Velocity = Vector2.MoveTowards(state.Velocity, targetVelocity,
                Mathf.Max(0f, settings.AccelerationMetresPerSecondSquared) * dt);
            state.Position += state.Velocity * dt;
        }

        public static float SpeedAtHeading(float heading, Vector2 windVelocity, SailingNavigationSettings settings)
        {
            settings = settings.Validated();
            float windSpeed = windVelocity.magnitude;
            if (windSpeed <= .1f) return 0f;
            float angle = Vector2.Angle(-windVelocity, Direction(heading));
            float minimum = Mathf.Clamp(settings.MinimumWindAngleDegrees, 0f, 89f);
            if (angle <= minimum) return 0f;
            float efficiency = angle < 90f
                ? Mathf.SmoothStep(0f, 1f, (angle - minimum) / (90f - minimum))
                : Mathf.Lerp(1f, .75f, (angle - 90f) / 90f);
            return windSpeed * Mathf.Max(0f, settings.WindSpeedRatio)
                * Mathf.Max(0f, settings.ForwardSpeedMultiplier) * efficiency;
        }

        private static float TackAngle(SailingNavigationSettings settings) => Mathf.Clamp(
            Mathf.Clamp(settings.MinimumWindAngleDegrees, 0f, 89f) + Mathf.Max(1f, settings.TackMarginDegrees), 1f, 89.9f);

        private static float Bearing(Vector2 direction) => Mathf.Repeat(
            Mathf.Atan2(direction.x, direction.y) * Mathf.Rad2Deg, 360f);

        private static Vector2 Direction(float heading)
        {
            float angle = heading * Mathf.Deg2Rad;
            return new Vector2(Mathf.Sin(angle), Mathf.Cos(angle));
        }

        private static Vector2 Rotate(Vector2 direction, float degrees)
        {
            float angle = degrees * Mathf.Deg2Rad;
            float sin = Mathf.Sin(angle);
            float cos = Mathf.Cos(angle);
            return new Vector2(direction.x * cos + direction.y * sin, direction.y * cos - direction.x * sin);
        }
    }
}
