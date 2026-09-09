using System;
using System.IO;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Interop
{
    internal static class CaveNative
    {
        private const string Library = "motu";
        internal const uint AlgorithmRevision = 11;
        [Serializable, StructLayout(LayoutKind.Sequential)]
        internal struct Options
        {
            public uint enabled;
            public uint seedOffset;
            public uint maximumCaves;
            public uint candidateLimit;
            public float spacing;
            public float entranceWidth;
            public float entranceHeight;
            public float minimumFaceSlope;
            public float minimumFaceHeight;
            public float approachSlope;
            public float approachLength;
            public float sideMargin;
            public float approachStep;
            public float routeRadius;
            public float roofCover;
            public float sideCover;
            public float transitionLength;
            public float lengthMin;
            public float lengthMax;
            public float widthMin;
            public float widthMax;
            public float heightMin;
            public float heightMax;
            public float floorSlope;
            public float chamberWidth;
            public float chamberHeight;
            public float broadAmplitude;
            public float broadPeriod;
            public float fineAmplitude;
            public float finePeriod;
            public float floorRoughness;
            public float seaClearance;
            public float voxelSize;
            internal void Write(BinaryWriter writer)
            {
                writer.Write(enabled);
                writer.Write(seedOffset);
                writer.Write(maximumCaves);
                writer.Write(candidateLimit);
                writer.Write(spacing);
                writer.Write(entranceWidth);
                writer.Write(entranceHeight);
                writer.Write(minimumFaceSlope);
                writer.Write(minimumFaceHeight);
                writer.Write(approachSlope);
                writer.Write(approachLength);
                writer.Write(sideMargin);
                writer.Write(approachStep);
                writer.Write(routeRadius);
                writer.Write(roofCover);
                writer.Write(sideCover);
                writer.Write(transitionLength);
                writer.Write(lengthMin);
                writer.Write(lengthMax);
                writer.Write(widthMin);
                writer.Write(widthMax);
                writer.Write(heightMin);
                writer.Write(heightMax);
                writer.Write(floorSlope);
                writer.Write(chamberWidth);
                writer.Write(chamberHeight);
                writer.Write(broadAmplitude);
                writer.Write(broadPeriod);
                writer.Write(fineAmplitude);
                writer.Write(finePeriod);
                writer.Write(floorRoughness);
                writer.Write(seaClearance);
                writer.Write(voxelSize);
            }
        }
        [Serializable, StructLayout(LayoutKind.Sequential)]
        internal struct NetworkOptions
        {
            public uint maximumBranches;
            public float branchLengthMin, branchLengthMax, chamberScale;
            internal void Write(BinaryWriter writer)
            {
                writer.Write(maximumBranches);
                writer.Write(branchLengthMin);
                writer.Write(branchLengthMax);
                writer.Write(chamberScale);
            }
        }
        [Serializable, StructLayout(LayoutKind.Sequential)]
        internal struct WalkOptions
        {
            public uint enabled;
            public float endProbability, branchProbability, stepMetres, turnDegrees, widthVariation;
            internal void Write(BinaryWriter writer)
            {
                writer.Write(enabled); writer.Write(endProbability); writer.Write(branchProbability);
                writer.Write(stepMetres); writer.Write(turnDegrees); writer.Write(widthVariation);
            }
        }
        [StructLayout(LayoutKind.Sequential)]
        internal struct Info
        {
            public ulong id;
            public Vector3 entrance;
            public Vector2 inward;
            public Vector3 chamber;
            public Vector2 minimum, maximum;
            public uint chunkCount;
        }
        [StructLayout(LayoutKind.Sequential)]
        internal struct Stats
        {
            public uint examined, approachRejected, faceRejected, coverRejected;
            public uint hazardRejected, spacingRejected, accepted;
            public override string ToString() => $"Caves: {accepted}; examined {examined}; rejected approach {approachRejected}, face {faceRejected}, cover/geometry {coverRejected}, hazards {hazardRejected}, spacing {spacingRejected}";
        }
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern uint CaveAlgorithmRevision();
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern IntPtr CreateMotuWithCaves(int seed,
            ref MotuNative.Options options, ref MotuNative.ForestOptions forest,
            ref MotuNative.ReedOptions reeds, ref MotuNative.FernOptions ferns, ref Options caves);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern IntPtr CreateMotuWithCaveNetworks(int seed,
            ref MotuNative.Options options, ref MotuNative.ForestOptions forest,
            ref MotuNative.ReedOptions reeds, ref MotuNative.FernOptions ferns, ref Options caves, ref NetworkOptions network);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern IntPtr CreateMotuWithCaveWalks(int seed,
            ref MotuNative.Options options, ref MotuNative.ForestOptions forest,
            ref MotuNative.ReedOptions reeds, ref MotuNative.FernOptions ferns, ref Options caves, ref NetworkOptions network, ref WalkOptions walk);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern uint GetCaveBranchCount(IntPtr handle, uint cave);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern uint GetCaveBranchNodeCount(IntPtr handle, uint cave, uint branch);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern byte GetCaveBranchNode(IntPtr handle, uint cave, uint branch, uint node, out Vector3 position);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern uint GetCaveCount(IntPtr handle);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern byte GetCaveStats(IntPtr handle, out Stats stats);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern byte GetCaveInfo(IntPtr handle, uint cave, out Info info);
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)] internal static extern byte CreateCaveMesh(IntPtr handle, uint cave, uint chunk, out MotuNative.ExportMesh mesh);
    }
}
