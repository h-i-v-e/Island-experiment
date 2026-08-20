using System;
using UnityEngine;

internal sealed class RockDecorationIndex
{
    internal const int Resolution = TerrainTileStreamer.Lod1Resolution;

    private readonly IslandPreparedRockDecoration[] candidates;
    private readonly int[] cellOffsets;
    private readonly int[] candidateOrder;
    private readonly float worldSize;

    internal RockDecorationIndex(
        IslandPreparedRockDecoration[] candidates,
        float worldSize)
    {
        this.candidates = candidates ?? Array.Empty<IslandPreparedRockDecoration>();
        this.worldSize = worldSize;
        var cellCount = Resolution * Resolution;
        var counts = new int[cellCount];
        for (var index = 0; index < this.candidates.Length; index++)
        {
            counts[CellIndex(this.candidates[index].position)]++;
        }

        cellOffsets = new int[cellCount + 1];
        for (var cell = 0; cell < cellCount; cell++)
        {
            cellOffsets[cell + 1] = cellOffsets[cell] + counts[cell];
        }

        candidateOrder = new int[this.candidates.Length];
        var cursors = new int[cellCount];
        Array.Copy(cellOffsets, cursors, cellCount);
        for (var index = 0; index < this.candidates.Length; index++)
        {
            var cell = CellIndex(this.candidates[index].position);
            candidateOrder[cursors[cell]++] = index;
        }
    }

    internal int Count => candidates.Length;
    internal IslandPreparedRockDecoration CandidateAt(int index) => candidates[index];
    internal int CandidateIndexAt(int orderIndex) => candidateOrder[orderIndex];

    internal void GetCellRange(int x, int y, out int start, out int end)
    {
        var cell = y * Resolution + x;
        start = cellOffsets[cell];
        end = cellOffsets[cell + 1];
    }

    internal Vector2Int CellAt(Vector3 worldPosition)
    {
        return new Vector2Int(
            CellCoordinate(worldPosition.x),
            CellCoordinate(worldPosition.z));
    }

    private int CellIndex(Vector3 worldPosition)
    {
        return CellCoordinate(worldPosition.z) * Resolution
            + CellCoordinate(worldPosition.x);
    }

    private int CellCoordinate(float coordinate)
    {
        return Mathf.Clamp(
            Mathf.FloorToInt((coordinate / worldSize + 0.5f) * Resolution),
            0,
            Resolution - 1);
    }
}
