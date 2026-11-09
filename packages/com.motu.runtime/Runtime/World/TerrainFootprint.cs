using UnityEngine;

namespace Motu.World
{
    /// <summary>A rectangle in world XZ. Size contains full dimensions in metres;
    /// yaw follows Unity's rotation around world Y. Vertical position is irrelevant.</summary>
    public readonly struct TerrainFootprint
    {
        public Vector2 Center { get; }
        public Vector2 Size { get; }
        public float YawDegrees { get; }

        public TerrainFootprint(Vector2 center, Vector2 size, float yawDegrees = 0f)
        {
            Center = center;
            Size = size;
            YawDegrees = yawDegrees;
        }

        public TerrainFootprint(Rect rectangle, float yawDegrees = 0f)
            : this(rectangle.center, rectangle.size, yawDegrees) { }
    }
}
