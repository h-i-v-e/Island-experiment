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
typedef struct { float x, y, z, w; } Vector4Export;
typedef struct { float x, y; } Vector2Export;
/* Cave ABI revision 1: dimensions in metres, angles in degrees. */
typedef struct {
    uint32_t enabled;
    uint32_t seed_offset;
    uint32_t maximum_caves;
    uint32_t candidate_limit;
    float spacing;
    float entrance_width;
    float entrance_height;
    float minimum_face_slope;
    float minimum_face_height;
    float approach_slope;
    float approach_length;
    float side_margin;
    float approach_step;
    float route_radius;
    float roof_cover;
    float side_cover;
    float transition_length;
    float length_min;
    float length_max;
    float width_min;
    float width_max;
    float height_min;
    float height_max;
    float floor_slope;
    float chamber_width;
    float chamber_height;
    float broad_amplitude;
    float broad_period;
    float fine_amplitude;
    float fine_period;
    float floor_roughness;
    float sea_clearance;
    float voxel_size;
} MotuCaveOptions;
typedef struct {
    uint64_t id;
    Vector3Export entrance;
    Vector2Export inward;
    Vector3Export chamber;
    Vector2Export minimum, maximum;
    uint32_t chunk_count;
} MotuCaveInfo;
typedef struct {
    uint32_t examined, approach_rejected, face_rejected, cover_rejected;
    uint32_t hazard_rejected, spacing_rejected, accepted;
} MotuCaveStats;
typedef struct {
    float maxZ, waterRatio, slopeMultiplier, coastalSlopeMultiplier;
    float continentalNoiseFrequency, detailNoiseFrequency;
    float hydraulicErosionStrength, hydraulicDepositionStrength;
    float hydraulicDepositionSlopeDegrees;
    float riverSourceCatchmentHectares, riverSourceSteepCatchmentMultiplier;
    float riverSourceElevationBoost;
    float riverSourceWidthMetres, riverMaximumWidthMetres;
    float riverSourceDepthMetres, riverMaximumDepthMetres;
    float continentalNoiseStrength, detailNoiseStrength, landMassOffset;
    float initialSoilDepthMetres;
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
typedef struct {
    float bankWidthMetres, patchSizeMetres, coverageThreshold, spacingMetres;
    float rushRatio, minimumHeightMetres, maximumHeightMetres, maximumSlopeDegrees;
} MotuReedOptions;
typedef struct {
    float barkClearanceMetres, outerRadiusMetres, spacingMetres, patchSizeMetres;
    float coverageThreshold, minimumLengthMetres, maximumLengthMetres, maximumSlopeDegrees;
} MotuFernOptions;
typedef struct { const Vector3Export *data; int32_t length; } Vector3ExportArray;
typedef struct { const Vector4Export *data; int32_t length; } Vector4ExportArray;
typedef struct { const Vector2Export *data; int32_t length; } Vector2ExportArray;
typedef struct { const int32_t *data; int32_t length; } TriangleExportArray;
typedef struct { Vector3Export min, max; } ExportArea;
typedef struct {
    void *handle;
    Vector3ExportArray vertices, normals;
    TriangleExportArray triangles;
    Vector2ExportArray uv;
    /* RGBA: bedrock/forced rock, loose cover, river bed, sea proximity. */
    Vector4ExportArray material;
    Vector2ExportArray environment;
} ExportMesh;
typedef struct {
    void *handle;
    Vector3ExportArray vertices, normals;
    TriangleExportArray triangles;
    Vector2ExportArray uv;
    Vector4ExportArray material;
    Vector2ExportArray environment;
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
    float halfWidth, drop;
} WaterfallFootExport;
typedef struct {
    void *handle;
    const WaterfallFootExport *data;
    int32_t length;
} ExportWaterfallFeet;
typedef struct { int32_t width, height; float *data; float seaLevel; } ExportHeightMapWithSeaLevel;
typedef struct {
    float dirtColour[3], stoneColour[3], sandColour[3];
} MotuMaterialInputs;
typedef struct {
    uint32_t width, height;
    /* normalConvention: 0 OpenGL, 1 DirectX. materialMask bits: rock 0x01,
       river bed 0x02, forest floor 0x04, fallen stones 0x08, dirt 0x10,
       beach 0x20, tree bark 0x40. */
    uint8_t normalConvention, materialMask;
    uint8_t reserved[2];
} MotuMaterialBakeOptions;
typedef struct { uint64_t low, high; } MotuMaterialRevision;
typedef struct { const uint8_t *data; int32_t length; } ByteExportArray;
typedef struct {
    int32_t width, height;
    float physicalTileWidthMetres, physicalTileHeightMetres;
    float minimumHeight, maximumHeight, baseHeight;
    ByteExportArray albedoRgb, normalRgb, heightR16, occlusion;
} ExportMaterialTexture;
typedef struct {
    void *handle;
    ExportMaterialTexture dirt, forestFloor, rock, riverBed, beach, fallenStones, treeBark;
} ExportMaterialTextureSet;
typedef struct { Vector3ExportArray trees, bushes; } ExportDecoration;
typedef struct { int32_t offset; float scale; } TreeMeshPrototype;
typedef struct { const TreeMeshPrototype *prototypes; int32_t length; } TreeMeshPrototypes;
typedef struct { ExportMesh mesh; int32_t *offsets; } ExportTreeBillboards;
typedef struct { ExportTreeBillboards octants[8]; void *offsetsHandle; } ExportTreeBillboardsArray;

MOTU_EXPORT void *CreateMotu(int32_t seed, const MotuOptions *options);
MOTU_EXPORT void *CreateMotuWithForest(int32_t seed, const MotuOptions *options,
                                        const MotuForestOptions *forestOptions);
MOTU_EXPORT void *CreateMotuWithForestAndReeds(int32_t seed, const MotuOptions *options,
                                               const MotuForestOptions *forestOptions,
                                               const MotuReedOptions *reedOptions);
MOTU_EXPORT void *CreateMotuWithForestReedsAndFerns(int32_t seed, const MotuOptions *options,
                                                    const MotuForestOptions *forestOptions,
                                                    const MotuReedOptions *reedOptions,
                                                    const MotuFernOptions *fernOptions);
MOTU_EXPORT void *LoadMotu(const char *filePath);
MOTU_EXPORT void SaveMotu(const void *handle, const char *filePath);
MOTU_EXPORT void ReleaseMotu(void *handle);
MOTU_EXPORT MotuMaterialRevision GetMotuRuntimeMaterialRevision(void);
MOTU_EXPORT uint8_t BakeMotuMaterialTextures(const MotuMaterialInputs *inputs,
                                              const MotuMaterialBakeOptions *options,
                                              ExportMaterialTextureSet *output);
MOTU_EXPORT void ReleaseMaterialTextureSet(ExportMaterialTextureSet *output);
MOTU_EXPORT void CreateProceduralTree(int32_t seed, ExportMesh *lod0Wood,
                                      ExportMesh *lod0Foliage, ExportMesh *lod1Wood,
                                      ExportMesh *lod1Foliage);
MOTU_EXPORT void CreateSkyDome(ExportMesh *output);
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
MOTU_EXPORT void CreateReedMeshGrid(const void *handle, ExportMeshGrid *output);
MOTU_EXPORT void CreateFernMeshGrid(const void *handle, ExportMeshGrid *output);
MOTU_EXPORT void CreateWaterfallFeet(const void *handle, ExportWaterfallFeet *output);
MOTU_EXPORT void ReleaseWaterfallFeet(ExportWaterfallFeet *output);
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

MOTU_EXPORT uint32_t CaveAlgorithmRevision(void);
MOTU_EXPORT void* CreateMotuWithCaves(int32_t seed, const MotuOptions* options,
    const MotuForestOptions* forest, const MotuReedOptions* reeds,
    const MotuFernOptions* ferns, const MotuCaveOptions* caves);
MOTU_EXPORT uint32_t GetCaveCount(const void* handle);
MOTU_EXPORT uint8_t GetCaveStats(const void* handle, MotuCaveStats* output);
MOTU_EXPORT uint8_t GetCaveInfo(const void* handle, uint32_t cave, MotuCaveInfo* output);
MOTU_EXPORT uint8_t CreateCaveMesh(const void* handle, uint32_t cave, uint32_t chunk, ExportMesh* output);
/* Release every successful CreateCaveMesh export with ReleaseMesh. */

#ifdef __cplusplus
}
#endif

#endif
