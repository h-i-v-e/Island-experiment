using System;
using System.Collections.Generic;
using UnityEngine;

[DisallowMultipleComponent]
public sealed class GridIslandGenerationRequestFactory : MonoBehaviour,
    IIslandGenerationRequestFactory
{
    [Serializable]
    public sealed class FixedIsland
    {
        [SerializeField] private Vector2Int gridPosition;
        [SerializeField] private IslandConfiguration configuration;
        [SerializeField] private string stableId;

        public Vector2Int GridPosition => gridPosition;
        public IslandConfiguration Configuration => configuration;
        public string StableId => stableId;

        public FixedIsland(
            Vector2Int gridPosition,
            IslandConfiguration configuration,
            string stableId = null)
        {
            this.gridPosition = gridPosition;
            this.configuration = configuration;
            this.stableId = stableId;
        }
    }

    [Header("Island Configurations")]
    [Tooltip("Configuration used by fixed cells without an override and by generated cells.")]
    [SerializeField] private IslandConfiguration defaultConfiguration;
    [Tooltip("Cells that always contain an island. A cell-specific configuration is optional.")]
    [SerializeField] private List<FixedIsland> fixedIslands = new List<FixedIsland>();

    [Header("Unlisted Cells")]
    [Tooltip("Allow cells not listed above to contain procedurally selected islands.")]
    [SerializeField] private bool generateUnlistedCells = true;
    [Range(0f, 1f)] [SerializeField] private float unlistedCellOccupancy = 0.32f;
    [Tooltip("Deterministic parameter variation applied independently to every returned request.")]
    [SerializeField] private IslandParameterVariationSettings parameterVariation =
        new IslandParameterVariationSettings();

    public int FixedIslandCount => fixedIslands.Count;
    public bool GeneratesUnlistedCells => generateUnlistedCells;
    public float UnlistedCellOccupancy => unlistedCellOccupancy;
    public IslandConfiguration DefaultConfiguration => defaultConfiguration;

    public IslandGenerationRequest CreateIslandGenerationRequest(
        int randomSeed,
        Vector2Int islandGridPosition)
    {
        var fixedIsland = FindFixedIsland(islandGridPosition);
        if (fixedIsland == null
            && (!generateUnlistedCells || !IsOccupied(randomSeed)))
        {
            return null;
        }

        var configuration = fixedIsland?.Configuration ?? defaultConfiguration;
        if (configuration == null)
        {
            throw new InvalidOperationException(
                $"Island cell {islandGridPosition} has no IslandConfiguration.");
        }
        return new IslandGenerationRequest(
            randomSeed,
            islandGridPosition,
            configuration,
            parameterVariation,
            fixedIsland?.StableId);
    }

    public void Configure(
        IslandConfiguration configuration,
        bool populateUnlistedCells,
        float unlistedOccupancy)
    {
        defaultConfiguration = configuration != null
            ? configuration
            : throw new ArgumentNullException(nameof(configuration));
        generateUnlistedCells = populateUnlistedCells;
        unlistedCellOccupancy = Mathf.Clamp01(unlistedOccupancy);
    }

    public void SetFixedIslands(params FixedIsland[] definitions)
    {
        fixedIslands.Clear();
        if (definitions != null)
        {
            fixedIslands.AddRange(definitions);
        }
        ValidateFixedIslands();
    }

    private FixedIsland FindFixedIsland(Vector2Int gridPosition)
    {
        foreach (var fixedIsland in fixedIslands)
        {
            if (fixedIsland != null && fixedIsland.GridPosition == gridPosition)
            {
                return fixedIsland;
            }
        }
        return null;
    }

    private bool IsOccupied(int randomSeed)
    {
        if (unlistedCellOccupancy <= 0f)
        {
            return false;
        }
        if (unlistedCellOccupancy >= 1f)
        {
            return true;
        }
        var hash = Mix(unchecked((uint)randomSeed));
        var sample = (hash & 0x00ffffffu) / 16777216f;
        return sample < unlistedCellOccupancy;
    }

    private void OnValidate()
    {
        unlistedCellOccupancy = Mathf.Clamp01(unlistedCellOccupancy);
        ValidateFixedIslands();
    }

    private void ValidateFixedIslands()
    {
        var occupied = new HashSet<Vector2Int>();
        foreach (var fixedIsland in fixedIslands)
        {
            if (fixedIsland != null && !occupied.Add(fixedIsland.GridPosition))
            {
                throw new InvalidOperationException(
                    $"Island grid cell {fixedIsland.GridPosition} is defined more than once.");
            }
        }
    }

    private static uint Mix(uint value)
    {
        value ^= value >> 16;
        value *= 0x7feb352du;
        value ^= value >> 15;
        value *= 0x846ca68bu;
        value ^= value >> 16;
        return value;
    }
}
