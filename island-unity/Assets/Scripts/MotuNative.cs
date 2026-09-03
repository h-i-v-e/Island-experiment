using System;
using System.Runtime.InteropServices;

internal static class MotuNative
{
    private const string Library = "motu";

    [StructLayout(LayoutKind.Sequential)]
    internal struct Options
    {
        internal float maxZ;
        internal float waterRatio;
        internal float slopeMultiplier;
        internal float coastalSlopeMultiplier;
        // Reuses the two retired coastal-generation slots, preserving the
        // offsets of the existing active fields in the native ABI.
        internal float continentalNoiseFrequency;
        internal float detailNoiseFrequency;
        internal float hydraulicErosionStrength;
        internal float hydraulicDepositionStrength;
        internal float hydraulicDepositionSlopeDegrees;
        internal float riverSourceCatchmentHectares;
        internal float riverSourceSteepMultiplier;
        internal float riverSourceElevationBoost;
        internal float riverSourceWidthMetres;
        internal float riverMaximumWidthMetres;
        internal float riverSourceDepthMetres;
        internal float riverMaximumDepthMetres;
        internal float continentalNoiseStrength;
        internal float detailNoiseStrength;
        internal float landMassOffset;
    }

    // Forest controls are kept in a separate native block. The byte fields
    // intentionally use the platform's natural C/Rust alignment (three bytes
    // of padding before each following float).
    [StructLayout(LayoutKind.Sequential)]
    internal struct ForestOptions
    {
        internal float patchSizeMetres;
        internal float noiseThreshold;
        internal byte noiseOctaves;
        internal float snowlineMetres;
        internal byte prototypeCount;
        internal float minimumScale;
        internal float maximumScale;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ReedOptions
    {
        internal float bankWidthMetres;
        internal float patchSizeMetres;
        internal float coverageThreshold;
        internal float spacingMetres;
        internal float rushRatio;
        internal float minimumHeightMetres;
        internal float maximumHeightMetres;
        internal float maximumSlopeDegrees;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FernOptions
    {
        internal float barkClearanceMetres;
        internal float outerRadiusMetres;
        internal float spacingMetres;
        internal float patchSizeMetres;
        internal float coverageThreshold;
        internal float minimumLengthMetres;
        internal float maximumLengthMetres;
        internal float maximumSlopeDegrees;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Vector3Array
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Vector4Array
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Vector2Array
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct TriangleArray
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeVector3
    {
        internal float x;
        internal float y;
        internal float z;

        internal NativeVector3(float x, float y, float z)
        {
            this.x = x;
            this.y = y;
            this.z = z;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeVector2
    {
        internal float x;
        internal float y;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportArea
    {
        internal NativeVector3 min;
        internal NativeVector3 max;

        internal ExportArea(float minimumX, float minimumY, float maximumX, float maximumY)
        {
            min = new NativeVector3(minimumX, minimumY, float.MinValue);
            max = new NativeVector3(maximumX, maximumY, float.MaxValue);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMesh
    {
        internal IntPtr handle;
        internal Vector3Array vertices;
        internal Vector3Array normals;
        internal TriangleArray triangles;
        internal Vector2Array uv;
        internal Vector4Array material;
        internal Vector2Array environment;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMeshWithUv
    {
        internal IntPtr handle;
        internal Vector3Array vertices;
        internal Vector3Array normals;
        internal TriangleArray triangles;
        internal Vector2Array uv;
        internal Vector4Array material;
        internal Vector2Array environment;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMeshGrid
    {
        internal IntPtr handle;
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportSurfaceMaps
    {
        internal IntPtr handle;
        internal int width;
        internal int height;
        internal IntPtr normalRgb;
        internal IntPtr occlusion;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportSeaMask
    {
        internal IntPtr handle;
        internal int width;
        internal int height;
        internal IntPtr rg;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportCloudWeatherMap
    {
        internal IntPtr handle;
        internal int width;
        internal int height;
        internal IntPtr rgba;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct MaterialInputs
    {
        internal float dirtRed;
        internal float dirtGreen;
        internal float dirtBlue;
        internal float stoneRed;
        internal float stoneGreen;
        internal float stoneBlue;
        internal float sandRed;
        internal float sandGreen;
        internal float sandBlue;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct MaterialBakeOptions
    {
        internal uint width;
        internal uint height;
        internal byte normalConvention;
        internal byte materialMask;
        internal ushort reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ByteArray
    {
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMaterialTexture
    {
        internal int width;
        internal int height;
        internal float minimumHeight;
        internal float maximumHeight;
        internal float baseHeight;
        internal ByteArray albedoRgb;
        internal ByteArray normalRgb;
        internal ByteArray heightR16;
        internal ByteArray occlusion;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportMaterialTextureSet
    {
        internal IntPtr handle;
        internal ExportMaterialTexture dirt;
        internal ExportMaterialTexture forestFloor;
        internal ExportMaterialTexture rock;
        internal ExportMaterialTexture riverBed;
        internal ExportMaterialTexture beach;
        internal ExportMaterialTexture fallenStones;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct WaterfallFootExport
    {
        internal NativeVector3 position;
        internal NativeVector3 direction;
        internal float halfWidth;
        internal float drop;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportWaterfallFeet
    {
        internal IntPtr handle;
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ForestTrunkColliderExport
    {
        internal NativeVector3 bottom;
        internal NativeVector3 top;
        internal NativeVector2 owner;
        internal float radius;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportForestTrunkColliders
    {
        internal IntPtr handle;
        internal IntPtr data;
        internal int length;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportDecoration
    {
        internal Vector3Array trees;
        internal Vector3Array bushes;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ExportHeightMapWithSeaLevel
    {
        internal int width;
        internal int height;
        internal IntPtr data;
        internal float seaLevel;
    }

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateMotu(int seed, ref Options options);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateMotuWithForest(
        int seed,
        ref Options options,
        ref ForestOptions forestOptions);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateMotuWithForestAndReeds(
        int seed,
        ref Options options,
        ref ForestOptions forestOptions,
        ref ReedOptions reedOptions);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateMotuWithForestReedsAndFerns(
        int seed,
        ref Options options,
        ref ForestOptions forestOptions,
        ref ReedOptions reedOptions,
        ref FernOptions fernOptions);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMotu(IntPtr handle);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int SaveMotuSnapshot(IntPtr handle, string path);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr LoadMotuSnapshot(string path, out int status);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateProceduralTree(
        int seed,
        out ExportMesh lod0Wood,
        out ExportMesh lod0Foliage,
        out ExportMesh lod1Wood,
        out ExportMesh lod1Foliage);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateSkyDome(out ExportMesh output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern byte CreateCloudWeatherMap(
        int seed,
        int resolution,
        out ExportCloudWeatherMap output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseCloudWeatherMap(
        ref ExportCloudWeatherMap output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateMesh(
        IntPtr handle,
        IntPtr area,
        int lod,
        byte clampSides,
        out ExportMesh output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMesh(ref ExportMesh output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int lod,
        int divisions,
        byte clampSides,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMeshGrid(ref ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateRiverMesh(
        IntPtr handle,
        IntPtr area,
        out ExportMeshWithUv output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateRiverMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int divisions,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateRiverRockMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int divisions,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMeshWithUV(ref ExportMeshWithUv output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateForestWoodMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int visualLod,
        int divisions,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateForestFoliageMeshGrid(
        IntPtr handle,
        ref ExportArea area,
        int visualLod,
        int divisions,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateReedMeshGrid(
        IntPtr handle,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateFernMeshGrid(
        IntPtr handle,
        out ExportMeshGrid output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateForestTrunkColliders(
        IntPtr handle,
        out ExportForestTrunkColliders output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseForestTrunkColliders(
        ref ExportForestTrunkColliders output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateWaterfallFeet(
        IntPtr handle,
        out ExportWaterfallFeet output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseWaterfallFeet(ref ExportWaterfallFeet output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void GetDecoration(
        IntPtr handle,
        out ExportDecoration output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateHeightMap(IntPtr handle, int resolution);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseHeightMap(IntPtr map);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr CreateTerrainColliderHeightMap(
        IntPtr handle,
        int samplesPerTile);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseTerrainColliderHeightMap(IntPtr map);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateSurfaceMaps(
        IntPtr handle,
        int lod,
        int dimension,
        out ExportSurfaceMaps output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseSurfaceMaps(ref ExportSurfaceMaps output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CreateSeaMask(
        IntPtr handle,
        int dimension,
        out ExportSeaMask output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseSeaMask(ref ExportSeaMask output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern byte BakeMotuMaterialTextures(
        ref MaterialInputs inputs,
        ref MaterialBakeOptions options,
        out ExportMaterialTextureSet output);

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ReleaseMaterialTextureSet(
        ref ExportMaterialTextureSet output);
}
