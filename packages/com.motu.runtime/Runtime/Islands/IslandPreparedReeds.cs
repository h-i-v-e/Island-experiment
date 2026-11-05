using UnityEngine;

namespace Motu.Islands
{
    internal readonly struct IslandPreparedReedPlacement
    {
        internal readonly Vector3 localRoot, groundNormal;
        internal readonly float heightMetres, widthMetres;
        internal readonly Vector4 data;

        internal IslandPreparedReedPlacement(Vector3 localRoot, Vector3 groundNormal,
            float heightMetres, float widthMetres, Vector4 data)
        {
            this.localRoot = localRoot;
            this.groundNormal = groundNormal;
            this.heightMetres = heightMetres;
            this.widthMetres = widthMetres;
            this.data = data;
        }
    }

    internal sealed class IslandPreparedReedTile
    {
        internal readonly IslandPreparedReedPlacement[] placements;
        internal IslandPreparedReedTile(IslandPreparedReedPlacement[] placements) => this.placements = placements;
    }
}
