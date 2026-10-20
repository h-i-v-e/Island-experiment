using System;
using UnityEngine;

namespace Motu.Gameplay
{
    /// <summary>Watertight transverse hull sections baked once from the collision mesh, in body-local metres.</summary>
    [CreateAssetMenu(menuName = "Motu/Ship Hull Geometry")]
    public sealed class ShipHullGeometry : ScriptableObject
    {
        [Serializable]
        public struct Section
        {
            public float station;
            public float length;
            public Vector2[] outline;
        }

        public struct ImmersedSection
        {
            public float area, wettedPerimeter, waterlineWidth, depth, minimumY, maximumY;
            public Vector3 centre;
        }

        [SerializeField] private Section[] sections = Array.Empty<Section>();
        public Section[] Sections => sections;

        // The caller owns the scratch buffer: no mesh reads or per-step allocations.
        public static ImmersedSection Immerse(Section section, Vector3 scale, Vector3 waterNormal,
            float waterHeight, Vector2[] clipped)
        {
            var result = new ImmersedSection();
            var points = section.outline;
            if (points == null || points.Length < 3) return result;
            float z = section.station * scale.z;
            float cut = waterHeight - waterNormal.z * z;
            int count = 0, crossings = 0;
            Vector2 firstCrossing = default, secondCrossing = default;
            float minimumY = float.MaxValue, maximumY = float.MinValue;
            for (int i = 0; i < points.Length; i++)
            {
                Vector2 a = Vector2.Scale(points[i], new Vector2(scale.x, scale.y));
                Vector2 b = Vector2.Scale(points[(i + 1) % points.Length], new Vector2(scale.x, scale.y));
                float da = waterNormal.x * a.x + waterNormal.y * a.y - cut;
                float db = waterNormal.x * b.x + waterNormal.y * b.y - cut;
                bool aWet = da <= 0f, bWet = db <= 0f;
                if (aWet) clipped[count++] = a;
                if (aWet != bWet)
                {
                    Vector2 edge = Vector2.LerpUnclamped(a, b, da / (da - db));
                    clipped[count++] = edge;
                    if (crossings++ == 0) firstCrossing = edge;
                    else secondCrossing = edge;
                    result.wettedPerimeter += Vector2.Distance(aWet ? a : b, edge);
                }
                else if (aWet) result.wettedPerimeter += Vector2.Distance(a, b);
            }
            if (count < 3) return default;
            double twiceArea = 0, cx = 0, cy = 0;
            for (int i = 0; i < count; i++)
            {
                Vector2 a = clipped[i], b = clipped[(i + 1) % count];
                double cross = (double)a.x * b.y - (double)b.x * a.y;
                twiceArea += cross;
                cx += (a.x + b.x) * cross;
                cy += (a.y + b.y) * cross;
                minimumY = Mathf.Min(minimumY, a.y);
                maximumY = Mathf.Max(maximumY, a.y);
            }
            if (Math.Abs(twiceArea) < .000001) return default;
            result.area = (float)(Math.Abs(twiceArea) * .5);
            result.centre = new Vector3((float)(cx / (3 * twiceArea)), (float)(cy / (3 * twiceArea)), z);
            result.depth = maximumY - minimumY;
            result.minimumY = minimumY;
            result.maximumY = maximumY;
            result.waterlineWidth = crossings >= 2 ? Vector2.Distance(firstCrossing, secondCrossing) : 0f;
            return result;
        }

        /// <summary>ITTC-1957 friction line plus laminar low-Reynolds behaviour, using SI units.</summary>
        public static float SkinFrictionCoefficient(float speed, float length, float viscosity)
        {
            float reynolds = Mathf.Max(speed * length / Mathf.Max(viscosity, .0000001f), 1f);
            if (reynolds < 500000f) return 1.328f / Mathf.Sqrt(reynolds);
            float log = Mathf.Log10(reynolds) - 2f;
            return .075f / (log * log);
        }
    }
}
