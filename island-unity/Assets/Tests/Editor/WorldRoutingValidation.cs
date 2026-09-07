using System;
using System.Collections.Generic;
using System.Threading;
using UnityEngine;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;

using Motu.Streaming;
using Motu.World;
using Motu.Rendering;
using static UnityEngine.Object;
using static Motu.World.IslandWorldManager;
namespace Motu.Editor
{
    internal static class WorldRoutingValidation
    {
        public static void ValidateRoutingPolicy()
        {
            if (NativeIslandHandle.ActiveCount != 0)
            {
                throw new InvalidOperationException(
                    "Native island handles remained allocated after generation validation.");
            }
            const float active = 6000f;
            const float hysteresis = 500f;
            const float generation = 10500f;
            const float unload = 12500f;
            const float discovery = 16000f;
            if (!(active < active + hysteresis
                && active + hysteresis < generation
                && generation < unload
                && unload < discovery))
            {
                throw new InvalidOperationException(
                    "Island residency and discovery thresholds are not correctly ordered.");
            }
            if (HorizontalDistance(new Vector3(3f, 80f, 4f), Vector3.zero) != 5f)
            {
                throw new InvalidOperationException(
                    "Island routing distance must ignore elevation.");
            }

            var expectedCell = new Vector2Int(-7, 11);
            var expectedCentre = CellCentre(expectedCell);
            if (WorldToCell(expectedCentre) != expectedCell
                || WorldToCell(new Vector3(999.99f, 0f, -999.99f)) != Vector2Int.zero
                || WorldToCell(new Vector3(1000f, 0f, -1000.01f))
                    != new Vector2Int(1, -1))
            {
                throw new InvalidOperationException(
                    "World positions do not map deterministically to 2 km island cells.");
            }

            var factoryObject = new GameObject("Island request factory validation");
            try
            {
                var factory = factoryObject.AddComponent<GridIslandGenerationRequestFactory>();
                var templateHeight = factory.GenerationSettings.MaximumHeightMetres;
                factory.Configure(8128, false, 0f);
                factory.SetFixedIslands(
                    new GridIslandGenerationRequestFactory.FixedIsland(
                        expectedCell,
                        "fixed-validation-island"));
                var fixedRequest = factory.CreateIslandGenerationRequest(expectedCell);
                var repeatedFixedRequest = factory.CreateIslandGenerationRequest(expectedCell);
                if (fixedRequest == null
                    || repeatedFixedRequest == null
                    || !factory.HasIsland(expectedCell)
                    || fixedRequest.IslandId != "fixed-validation-island"
                    || fixedRequest.RandomSeed != repeatedFixedRequest.RandomSeed
                    || fixedRequest.IslandGridPosition != expectedCell
                    || !Mathf.Approximately(fixedRequest.WorldSizeMetres, IslandSizeMetres)
                    || !Mathf.Approximately(
                        fixedRequest.Descriptor.EstimatedBoundingRadiusMetres,
                        IslandSizeMetres * 0.5f)
                    || factory.HasIsland(Vector2Int.zero)
                    || factory.CreateIslandGenerationRequest(Vector2Int.zero) != null)
                {
                    throw new InvalidOperationException(
                        "Fixed cells and open-sea cells are not controlled by the request factory.");
                }
                var generationBoundaryCentreDistance = 3000f
                    + fixedRequest.Descriptor.EstimatedBoundingRadiusMetres;
                if (!Mathf.Approximately(
                        DistanceFromIslandEdge(
                            generationBoundaryCentreDistance,
                            fixedRequest.Descriptor),
                        3000f))
                {
                    throw new InvalidOperationException(
                        "Island generation and unloading do not use the same edge-distance convention.");
                }
                if (!Mathf.Approximately(
                        factory.GenerationSettings.MaximumHeightMetres,
                        templateHeight))
                {
                    throw new InvalidOperationException(
                        "Creating an island request mutated the factory's settings template.");
                }

                factory.Configure(8128, true, 1f);
                var managedCell = new Vector2Int(3, -4);
                var firstManaged = factory.CreateIslandGenerationRequest(managedCell);
                var repeatedManaged = factory.CreateIslandGenerationRequest(managedCell);
                factory.Configure(9128, true, 1f);
                var otherManaged = factory.CreateIslandGenerationRequest(managedCell);
                if (firstManaged == null || repeatedManaged == null || otherManaged == null)
                {
                    throw new InvalidOperationException(
                        "The request factory did not create configured managed islands.");
                }
                if (!factory.HasIsland(managedCell))
                {
                    throw new InvalidOperationException(
                        "HasIsland disagrees with the request factory's occupied-cell policy.");
                }
                if (!Mathf.Approximately(
                        firstManaged.Profile.Generation.MaximumHeightMetres,
                        repeatedManaged.Profile.Generation.MaximumHeightMetres)
                    || !Mathf.Approximately(
                        firstManaged.Profile.Generation.WaterRatio,
                        repeatedManaged.Profile.Generation.WaterRatio))
                {
                    throw new InvalidOperationException(
                        "Factory-selected island variation is not deterministic.");
                }
                if (Mathf.Approximately(
                        firstManaged.Profile.Generation.MaximumHeightMetres,
                        otherManaged.Profile.Generation.MaximumHeightMetres)
                    && Mathf.Approximately(
                        firstManaged.Profile.Generation.WaterRatio,
                        otherManaged.Profile.Generation.WaterRatio))
                {
                    throw new InvalidOperationException(
                        "The request factory did not vary distinct island seeds.");
                }
            }
            finally
            {
                DestroyImmediate(factoryObject);
            }
        }
    }
}
