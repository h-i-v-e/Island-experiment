#ifndef MOTU_RUST_H
#define MOTU_RUST_H

#include <stdint.h>

#ifdef _WIN32
#define MOTU_EXPORT __declspec(dllimport)
#else
#define MOTU_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct { float x, y, z; } Vector3Export;
typedef struct { float x, y; } Vector2Export;
typedef struct {
    float maxZ, waterRatio, slopeMultiplier, coastalSlopeMultiplier;
    float coastalErosionStrength, beachFormationStrength;
    float hydraulicErosionStrength, hydraulicDepositionStrength;
    float hydraulicDepositionSlopeDegrees;
    float riverSourceCatchmentHectares, riverSourceSteepMultiplier;
    float riverSourceElevationBoost;
    float riverSourceWidthMetres, riverMaximumWidthMetres;
    float riverSourceDepthMetres, riverMaximumDepthMetres;
} MotuOptions;
/* Forest options use the same natural C layout as Rust's repr(C) block. */
typedef struct {
    float patchSizeMetres;
    float noiseThreshold;
    uint8_t noiseOctaves;
    float snowlineMetres;
    uint8_t prototypeCount;
    float minimumScale, maximumScale;
} MotuForestOptions;
typedef struct { const Vector3Export *data; int32_t length; } Vector3ExportArray;
typedef struct { const Vector2Export *data; int32_t length; } Vector2ExportArray;
typedef struct { const int32_t *data; int32_t length; } TriangleExportArray;
typedef struct { Vector3Export min, max; } ExportArea;
typedef struct {
    void *handle;
    Vector3ExportArray vertices, normals;
    TriangleExportArray triangles;
    Vector2ExportArray uv;
    /* RGB: bedrock/forced rock, loose cover, sea proximity (1 through 2 m, 0 at 20 m). */
    Vector3ExportArray material;
} ExportMesh;
typedef struct {
    void *handle;
    Vector3ExportArray vertices, normals;
    TriangleExportArray triangles;
    Vector2ExportArray uv;
    Vector3ExportArray material;
} ExportMeshWithUV;
typedef struct { ExportMesh *data; int32_t length; } ExportMeshArray;
typedef struct { void *handle; const ExportMesh *data; int32_t length; } ExportMeshGrid;
typedef struct {
    void *handle;
    int32_t width, height;
    const uint8_t *rg;
} ExportSeaMask;
typedef struct {
    Vector3Export position, direction;
    float strength;
} RiverEmitterExport;
typedef struct {
    void *handle;
    const RiverEmitterExport *data;
    int32_t length;
} ExportRiverEmitters;
typedef struct { int32_t width, height; float *data; float seaLevel; } ExportHeightMapWithSeaLevel;
typedef struct { Vector3ExportArray trees, bushes; } ExportDecoration;
typedef struct { int32_t offset; float scale; } TreeMeshPrototype;
typedef struct { const TreeMeshPrototype *prototypes; int32_t length; } TreeMeshPrototypes;
typedef struct { ExportMesh mesh; int32_t *offsets; } ExportTreeBillboards;
typedef struct { ExportTreeBillboards octants[8]; void *offsetsHandle; } ExportTreeBillboardsArray;

MOTU_EXPORT void *CreateMotu(int32_t seed, const MotuOptions *options);
MOTU_EXPORT void *CreateMotuWithForest(int32_t seed, const MotuOptions *options,
                                        const MotuForestOptions *forestOptions);
MOTU_EXPORT void *LoadMotu(const char *filePath);
MOTU_EXPORT void SaveMotu(const void *handle, const char *filePath);
MOTU_EXPORT void ReleaseMotu(void *handle);
MOTU_EXPORT void CreateProceduralTree(int32_t seed, ExportMesh *lod0Wood,
                                      ExportMesh *lod0Foliage, ExportMesh *lod1Wood,
                                      ExportMesh *lod1Foliage);
MOTU_EXPORT void GetDecoration(const void *handle, ExportDecoration *output);
MOTU_EXPORT void CreateMesh(const void *handle, const ExportArea *area, int32_t lod,
                            uint8_t clampSides, ExportMesh *output);
MOTU_EXPORT void CreateSupportMesh(const void *handle, const ExportArea *area, int32_t lod,
                                   ExportMesh *output);
MOTU_EXPORT void ReleaseMesh(ExportMesh *output);
MOTU_EXPORT void CreateMeshGrid(const void *handle, const ExportArea *area, int32_t lod,
                                int32_t divisions, uint8_t clampSides,
                                ExportMeshGrid *output);
MOTU_EXPORT void ReleaseMeshGrid(ExportMeshGrid *output);
MOTU_EXPORT void CreateRiverMesh(const void *handle, const ExportArea *area,
                                 ExportMeshWithUV *output);
MOTU_EXPORT void CreateRiverMeshGrid(const void *handle, const ExportArea *area,
                                     int32_t divisions, ExportMeshGrid *output);
MOTU_EXPORT void ReleaseMeshWithUV(ExportMeshWithUV *output);
MOTU_EXPORT void CreateForestWoodMeshGrid(const void *handle, const ExportArea *area,
                                          int32_t visualLod, int32_t divisions,
                                          ExportMeshGrid *output);
MOTU_EXPORT void CreateForestFoliageMeshGrid(const void *handle, const ExportArea *area,
                                             int32_t visualLod, int32_t divisions,
                                             ExportMeshGrid *output);
MOTU_EXPORT void CreateRiverEmitters(const void *handle, float sharpnessDegrees,
                                     float spacingMetres, ExportRiverEmitters *output);
MOTU_EXPORT void ReleaseRiverEmitters(ExportRiverEmitters *output);
MOTU_EXPORT ExportHeightMapWithSeaLevel *CreateHeightMap(const void *handle, int32_t resolution);
MOTU_EXPORT void ReleaseHeightMap(ExportHeightMapWithSeaLevel *map);
MOTU_EXPORT ExportHeightMapWithSeaLevel *CreateTerrainColliderHeightMap(
    const void *handle, int32_t samplesPerTile);
MOTU_EXPORT void ReleaseTerrainColliderHeightMap(ExportHeightMapWithSeaLevel *map);
MOTU_EXPORT uint8_t *CreateNormalMap(const void *handle, int32_t lod, int32_t dimension);
MOTU_EXPORT void ReleaseNormalMap(uint8_t *data);
MOTU_EXPORT uint8_t *CreateNormalMap3DC(const void *handle, int32_t lod, int32_t dimension);
MOTU_EXPORT void ReleaseNormalMap3DC(uint8_t *data);
MOTU_EXPORT uint32_t *ExportFoliageData(const void *handle, int32_t dimension);
MOTU_EXPORT void ReleaseFoliageData(uint32_t *data);
MOTU_EXPORT float *CreateSeaDepthMap(const void *handle, int32_t dimension);
MOTU_EXPORT void ReleaseSeaDepthMap(float *data);
MOTU_EXPORT void CreateSeaMask(const void *handle, int32_t dimension, ExportSeaMask *output);
MOTU_EXPORT void ReleaseSeaMask(ExportSeaMask *output);
MOTU_EXPORT void CreateTreeBillboards(const void *handle, const TreeMeshPrototypes *input,
                                      ExportTreeBillboardsArray *output);
MOTU_EXPORT void ReleaseTreeBillboards(ExportTreeBillboardsArray *output);
MOTU_EXPORT void ReleaseMeshes(ExportMeshArray *output);
MOTU_EXPORT void SetLogFile(const char *path);

#ifdef __cplusplus
}
#endif

#endif
