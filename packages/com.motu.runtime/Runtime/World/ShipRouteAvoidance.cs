using UnityEngine;

namespace Motu.World
{
    public readonly struct ShipRouteAvoidanceResult
    {
        public float HeadingDegrees { get; }
        public bool CanProceed { get; }
        // Negative is port, positive is starboard; preserve this while avoiding.
        public int TurnSign { get; }

        internal ShipRouteAvoidanceResult(float headingDegrees, bool canProceed, int turnSign)
        {
            HeadingDegrees = headingDegrees;
            CanProceed = canProceed;
            TurnSign = turnSign;
        }
    }

    /// <summary>Read-only, bounded local avoidance for loaded island terrain.</summary>
    public static class ShipRouteAvoidance
    {
        public static ShipRouteAvoidanceResult Evaluate(IslandWorldManager world, Vector2 position,
            float desiredHeadingDegrees, float speedMetresPerSecond, float seaLevel,
            float hullWidthMetres, float safeDraftMetres, float lookAheadMetres,
            Vector2 windVelocity, float minimumWindAngleDegrees, int preferredTurn = 0, float hullLengthMetres = 0f)
        {
            if (world == null || !Finite(position.x) || !Finite(position.y)
                || !Finite(desiredHeadingDegrees) || !Finite(speedMetresPerSecond) || speedMetresPerSecond < 0f
                || !Finite(seaLevel) || !Finite(hullWidthMetres) || hullWidthMetres <= 0f
                || !Finite(safeDraftMetres) || safeDraftMetres < 0f
                || !Finite(lookAheadMetres) || lookAheadMetres <= 0f
                || !Finite(windVelocity.x) || !Finite(windVelocity.y)
                || !Finite(hullLengthMetres) || hullLengthMetres < 0f
                || !Finite(minimumWindAngleDegrees) || minimumWindAngleDegrees < 0f
                || minimumWindAngleDegrees >= 180f)
                return new ShipRouteAvoidanceResult(desiredHeadingDegrees, false, 0);

            var firstSide = preferredTurn < 0 ? -1 : 1;
            for (var step = 0; step <= 12; step++)
            {
                int count = step == 0 ? 1 : 2;
                for (var side = 0; side < count; side++)
                {
                    int turn = step == 0 ? 0 : (side == 0 ? firstSide : -firstSide);
                    float heading = Mathf.Repeat(desiredHeadingDegrees + turn * step * 15f, 360f);
                    float radians = heading * Mathf.Deg2Rad;
                    var forward = new Vector2(Mathf.Sin(radians), Mathf.Cos(radians));
                    // Wind is velocity toward its destination; the no-go cone points against it.
                    if (windVelocity.sqrMagnitude > .0001f
                        && Vector2.Angle(forward, -windVelocity) < minimumWindAngleDegrees) continue;
                    if (IsHeadingClear(world, position, heading, seaLevel, hullWidthMetres,
                            safeDraftMetres, lookAheadMetres, hullLengthMetres))
                        return new ShipRouteAvoidanceResult(heading, true, turn);
                }
            }
            return new ShipRouteAvoidanceResult(desiredHeadingDegrees, false, 0);
        }

        /// <summary>Check one exact direction without searching alternatives or applying wind rules.
        /// Use for a velocity or turning safety guard, not as a wind-capable course.</summary>
        public static bool IsHeadingClear(IslandWorldManager world, Vector2 position, float headingDegrees,
            float seaLevel, float hullWidthMetres, float safeDraftMetres, float distanceMetres,
            float hullLengthMetres = 0f)
        {
            if (world == null || !Finite(position.x) || !Finite(position.y) || !Finite(headingDegrees)
                || !Finite(seaLevel) || !Finite(hullWidthMetres) || hullWidthMetres <= 0f
                || !Finite(safeDraftMetres) || safeDraftMetres < 0f
                || !Finite(distanceMetres) || distanceMetres <= 0f
                || !Finite(hullLengthMetres) || hullLengthMetres < 0f) return false;
            var radians = headingDegrees * Mathf.Deg2Rad;
            var forward = new Vector2(Mathf.Sin(radians), Mathf.Cos(radians));
            var footprint = new TerrainFootprint(position + forward * (distanceMetres * .5f),
                new Vector2(hullWidthMetres, distanceMetres + Mathf.Max(hullWidthMetres, hullLengthMetres)), headingDegrees);
            return world.IsShipFootprintClear(footprint, seaLevel, safeDraftMetres);
        }

        private static bool Finite(float value) => !float.IsNaN(value) && !float.IsInfinity(value);
    }
}
