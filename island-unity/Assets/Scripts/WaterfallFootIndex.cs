using System;
using UnityEngine;

internal sealed class WaterfallFootIndex
{
    internal const int Resolution = TerrainTileStreamer.Lod1Resolution;

    private readonly IslandPreparedWaterfallFoot[] feet;
    private readonly int[] cellOffsets;
    private readonly int[] candidateOrder;
    private readonly float worldSize;

    internal WaterfallFootIndex(
        IslandPreparedWaterfallFoot[] feet,
        float worldSize)
    {
        this.feet = feet ?? Array.Empty<IslandPreparedWaterfallFoot>();
        this.worldSize = worldSize;
        var cellCount = Resolution * Resolution;
        var counts = new int[cellCount];
        for (var index = 0; index < this.feet.Length; index++)
        {
            counts[CellIndex(this.feet[index].position)]++;
        }

        cellOffsets = new int[cellCount + 1];
        for (var cell = 0; cell < cellCount; cell++)
        {
            cellOffsets[cell + 1] = cellOffsets[cell] + counts[cell];
        }
        candidateOrder = new int[this.feet.Length];
        var cursors = new int[cellCount];
        Array.Copy(cellOffsets, cursors, cellCount);
        for (var index = 0; index < this.feet.Length; index++)
        {
            var cell = CellIndex(this.feet[index].position);
            candidateOrder[cursors[cell]++] = index;
        }
    }

    internal int Count => feet.Length;
    internal IslandPreparedWaterfallFoot FootAt(int index) => feet[index];
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

    internal void CellsIntersecting(
        Vector3 worldPosition,
        float radius,
        out int minimumX,
        out int maximumX,
        out int minimumY,
        out int maximumY)
    {
        minimumX = CellCoordinate(worldPosition.x - radius);
        maximumX = CellCoordinate(worldPosition.x + radius);
        minimumY = CellCoordinate(worldPosition.z - radius);
        maximumY = CellCoordinate(worldPosition.z + radius);
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
