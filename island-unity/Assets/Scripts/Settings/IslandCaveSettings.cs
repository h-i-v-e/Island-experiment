using System;
using UnityEngine;
using Motu.Interop;

namespace Motu.Settings
{
    [Serializable]
    public sealed class IslandCaveSettings
    {
        internal IslandCaveSettings Copy() => (IslandCaveSettings)MemberwiseClone();
        [Header("Generation")]
        [SerializeField] private bool enabled = false;
        public bool Enabled { get => enabled; set => enabled = value; }
        [SerializeField] private int seedOffset = 0;
        public int SeedOffset { get => seedOffset; set => seedOffset = value; }
        [Range(0, 4), Tooltip("Maximum accepted caves. Islands without a safe entrance may have none.")]
        [SerializeField] private int maximumCaves = 1;
        public int MaximumCaves { get => maximumCaves; set => maximumCaves = value; }
        [Range(1, 4096), Tooltip("Steep faces considered per island. More candidates improve coverage of complex cliffs at additional generation cost.")]
        [SerializeField] private int candidateLimit = 2048;
        public int CandidateLimit { get => candidateLimit; set => candidateLimit = value; }
        [SerializeField] private float spacing = 100.0f;
        public float Spacing { get => spacing; set => spacing = value; }
        [Header("Entrance")]
        [SerializeField] private float entranceWidth = 4.0f;
        public float EntranceWidth { get => entranceWidth; set => entranceWidth = value; }
        [SerializeField] private float entranceHeight = 3.0f;
        public float EntranceHeight { get => entranceHeight; set => entranceHeight = value; }
        [SerializeField] private float minimumFaceSlope = 60.0f;
        public float MinimumFaceSlope { get => minimumFaceSlope; set => minimumFaceSlope = value; }
        [SerializeField] private float minimumFaceHeight = 6.0f;
        public float MinimumFaceHeight { get => minimumFaceHeight; set => minimumFaceHeight = value; }
        [SerializeField] private float approachSlope = 15.0f;
        public float ApproachSlope { get => approachSlope; set => approachSlope = value; }
        [SerializeField] private float approachLength = 6.0f;
        public float ApproachLength { get => approachLength; set => approachLength = value; }
        [SerializeField] private float sideMargin = 1.0f;
        public float SideMargin { get => sideMargin; set => sideMargin = value; }
        [Tooltip("Local height variation allowed in addition to the approach slope, in metres. Used for the landing and its connected walking route.")]
        [SerializeField] private float approachStep = 0.2f;
        public float ApproachStep { get => approachStep; set => approachStep = value; }
        [SerializeField] private float routeRadius = 20.0f;
        public float RouteRadius { get => routeRadius; set => routeRadius = value; }
        [Header("Rock cover")]
        [SerializeField] private float roofCover = 3.0f;
        public float RoofCover { get => roofCover; set => roofCover = value; }
        [SerializeField] private float sideCover = 3.0f;
        public float SideCover { get => sideCover; set => sideCover = value; }
        [Tooltip("Distance from the walkable mouth to full tunnel cover. The cliff must reach the minimum face height across the opening by this distance.")]
        [SerializeField] private float transitionLength = 6.0f;
        public float TransitionLength { get => transitionLength; set => transitionLength = value; }
        [Header("Passage and chamber")]
        [SerializeField] private float lengthMin = 30.0f;
        public float LengthMin { get => lengthMin; set => lengthMin = value; }
        [SerializeField] private float lengthMax = 60.0f;
        public float LengthMax { get => lengthMax; set => lengthMax = value; }
        [SerializeField] private float widthMin = 3.0f;
        public float WidthMin { get => widthMin; set => widthMin = value; }
        [SerializeField] private float widthMax = 5.0f;
        public float WidthMax { get => widthMax; set => widthMax = value; }
        [SerializeField] private float heightMin = 3.0f;
        public float HeightMin { get => heightMin; set => heightMin = value; }
        [SerializeField] private float heightMax = 4.0f;
        public float HeightMax { get => heightMax; set => heightMax = value; }
        [Tooltip("Maximum permitted slope during final mesh traversal checks. The initial layout has a level floor with bounded roughness.")]
        [SerializeField] private float floorSlope = 12.0f;
        public float FloorSlope { get => floorSlope; set => floorSlope = value; }
        [SerializeField] private float chamberWidth = 10.0f;
        public float ChamberWidth { get => chamberWidth; set => chamberWidth = value; }
        [SerializeField] private float chamberHeight = 6.0f;
        public float ChamberHeight { get => chamberHeight; set => chamberHeight = value; }
        [Header("Shape noise")]
        [SerializeField] private float broadAmplitude = 0.5f;
        public float BroadAmplitude { get => broadAmplitude; set => broadAmplitude = value; }
        [SerializeField] private float broadPeriod = 8.0f;
        public float BroadPeriod { get => broadPeriod; set => broadPeriod = value; }
        [SerializeField] private float fineAmplitude = 0.15f;
        public float FineAmplitude { get => fineAmplitude; set => fineAmplitude = value; }
        [SerializeField] private float finePeriod = 2.0f;
        public float FinePeriod { get => finePeriod; set => finePeriod = value; }
        [SerializeField] private float floorRoughness = 0.08f;
        public float FloorRoughness { get => floorRoughness; set => floorRoughness = value; }
        [SerializeField] private float seaClearance = 5.0f;
        public float SeaClearance { get => seaClearance; set => seaClearance = value; }
        [Header("Meshing")]
        [Range(.25f, 1f), Tooltip("Target passage mesh spacing in metres. Smaller values add rings and increase mesh cost.")]
        [SerializeField] private float voxelSize = 0.5f;
        public float VoxelSize { get => voxelSize; set => voxelSize = value; }

        internal CaveNative.Options ToNative()
        {
            var result = new CaveNative.Options
            {
                enabled = enabled ? 1u : 0u,
                seedOffset = checked((uint)seedOffset),
                maximumCaves = checked((uint)maximumCaves),
                candidateLimit = checked((uint)candidateLimit),
                spacing = spacing,
                entranceWidth = entranceWidth,
                entranceHeight = entranceHeight,
                minimumFaceSlope = minimumFaceSlope,
                minimumFaceHeight = minimumFaceHeight,
                approachSlope = approachSlope,
                approachLength = approachLength,
                sideMargin = sideMargin,
                approachStep = approachStep,
                routeRadius = routeRadius,
                roofCover = roofCover,
                sideCover = sideCover,
                transitionLength = transitionLength,
                lengthMin = lengthMin,
                lengthMax = lengthMax,
                widthMin = widthMin,
                widthMax = widthMax,
                heightMin = heightMin,
                heightMax = heightMax,
                floorSlope = floorSlope,
                chamberWidth = chamberWidth,
                chamberHeight = chamberHeight,
                broadAmplitude = broadAmplitude,
                broadPeriod = broadPeriod,
                fineAmplitude = fineAmplitude,
                finePeriod = finePeriod,
                floorRoughness = floorRoughness,
                seaClearance = seaClearance,
                voxelSize = voxelSize,
            };
            return result;
        }
    }
}
