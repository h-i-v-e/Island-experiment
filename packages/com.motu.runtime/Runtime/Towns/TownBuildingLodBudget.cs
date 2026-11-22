using System.Collections.Generic;
using UnityEngine;

namespace Motu.Towns
{
    internal sealed class TownBuildingLod
    {
        internal const int PaintedShell = 2;

        private readonly LODGroup group;
        private readonly Bounds localBounds;
        private readonly TownDecorationAnchors decorations;
        internal int Level { get; private set; } = PaintedShell;

        internal TownBuildingLod(LODGroup group, Bounds localBounds, TownDecorationAnchors decorations)
        {
            this.group = group;
            this.localBounds = localBounds;
            this.decorations = decorations;
            group.ForceLOD(PaintedShell);
        }

        internal float SqrDistanceTo(Vector3 viewer)
        {
            var shell = group.transform;
            var nearest = localBounds.ClosestPoint(shell.InverseTransformPoint(viewer));
            return (shell.TransformPoint(nearest) - viewer).sqrMagnitude;
        }

        internal void SetLevel(int level)
        {
            if (Level == level) return;
            group.ForceLOD(level);
            Level = level;
            decorations?.SetBudgetLevel(level);
        }
    }

    internal sealed class TownBuildingLodBudget
    {
        internal const int InteriorLimit = 4;
        internal const int ExteriorLimit = 24;
        internal const float InteriorRangeMetres = 18f;
        internal const float ExteriorRangeMetres = 80f;
        private const float RefreshSeconds = 0.15f;

        private readonly List<(TownBuildingLod building, float sqrDistance, int order)> candidates
            = new List<(TownBuildingLod, float, int)>();
        private float nextUpdate;

        internal bool BeginUpdate()
        {
            var now = Time.unscaledTime;
            if (now < nextUpdate) return false;
            nextUpdate = now + RefreshSeconds;
            candidates.Clear();
            return true;
        }

        internal void Consider(TownRenderer town, Vector3 viewer)
        {
            if (town == null) return;
            var buildings = town.BuildingLods;
            for (var i = 0; i < buildings.Count; i++)
            {
                var building = buildings[i];
                var distance = building.SqrDistanceTo(viewer);
                if (distance <= ExteriorRangeMetres * ExteriorRangeMetres)
                    candidates.Add((building, distance, candidates.Count));
                else
                    building.SetLevel(TownBuildingLod.PaintedShell);
            }
        }

        internal void Apply()
        {
            candidates.Sort((a, b) =>
            {
                var distance = a.sqrDistance.CompareTo(b.sqrDistance);
                return distance != 0 ? distance : a.order.CompareTo(b.order);
            });
            var interiors = 0;
            while (interiors < InteriorLimit && interiors < candidates.Count
                && candidates[interiors].sqrDistance <= InteriorRangeMetres * InteriorRangeMetres)
            {
                interiors++;
            }
            // Return decorations before the newly selected buildings rent theirs.
            for (var index = interiors; index < candidates.Count; index++)
                candidates[index].building.SetLevel(index - interiors < ExteriorLimit ? 1 : TownBuildingLod.PaintedShell);
            for (var index = 0; index < interiors; index++)
                candidates[index].building.SetLevel(0);
            candidates.Clear();
        }
    }
}
