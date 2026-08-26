using System;
using UnityEngine;
using UnityEngine.Rendering;

[ExecuteAlways]
public sealed class ProceduralTreePreview : MonoBehaviour
{
    private const float NativeWorldSizeMetres = 2000f;
    private const string WoodObjectName = "Wood";
    private const string FoliageObjectName = "Foliage";
    private const float ButtonOrbitDegrees = 20f;
    private static readonly Rect ControlPanel = new Rect(16f, 16f, 440f, 165f);

    [SerializeField] private int seed = 2018;
    [SerializeField] private Material woodMaterial;
    [SerializeField] private Material foliageMaterial;
    [SerializeField] private Camera previewCamera;
    [SerializeField] private OrbitCamera orbitCamera;

    private Mesh lod0WoodMesh;
    private Mesh lod1WoodMesh;
    private MeshFilter woodFilter;
    private MeshRenderer woodRenderer;
    private Mesh lod0FoliageMesh;
    private Mesh lod1FoliageMesh;
    private MeshFilter foliageFilter;
    private MeshRenderer foliageRenderer;
    private Material runtimeWoodMaterial;
    private Material runtimeFoliageMaterial;
    private Material runtimeLod0FoliageMaterial;
    private Texture3D surfaceNoiseTexture;
    private bool generating;
    private bool showingLod1;

    public int Seed
    {
        get => seed;
        set => seed = value;
    }

    public bool IsLod1Visible => showingLod1;

    private void Update()
    {
        if (runtimeFoliageMaterial != null && previewCamera != null)
        {
            var cameraPosition = previewCamera.transform.position;
            runtimeFoliageMaterial.SetVector("_GrassPlayerPosition", cameraPosition);
            runtimeLod0FoliageMaterial?.SetVector("_GrassPlayerPosition", cameraPosition);
        }
        if (Input.GetKeyDown(KeyCode.L))
        {
            ToggleLod();
        }
    }

    private void OnEnable()
    {
        if (woodMaterial != null && foliageMaterial != null)
        {
            EnsureRuntimeMaterials();
            Regenerate();
        }
    }

    private void OnDisable()
    {
        ReleaseGeneratedMeshes();
        ReleaseRuntimeMaterials();
    }

    public void Configure(
        Material wood,
        Material foliage,
        Camera camera,
        OrbitCamera orbit)
    {
        ReleaseRuntimeMaterials();
        woodMaterial = wood;
        foliageMaterial = foliage;
        previewCamera = camera;
        orbitCamera = orbit;
        if (isActiveAndEnabled && woodMaterial != null && foliageMaterial != null)
        {
            EnsureRuntimeMaterials();
        }
    }

    [ContextMenu("Regenerate Tree")]
    public void Regenerate()
    {
        if (generating || woodMaterial == null || foliageMaterial == null)
        {
            return;
        }
        EnsureRuntimeMaterials();
        generating = true;
        MotuNative.ExportMesh lod0Wood = default;
        MotuNative.ExportMesh lod0Foliage = default;
        MotuNative.ExportMesh lod1Wood = default;
        MotuNative.ExportMesh lod1Foliage = default;
        try
        {
            MotuNative.CreateProceduralTree(
                seed,
                out lod0Wood,
                out lod0Foliage,
                out lod1Wood,
                out lod1Foliage);
            ValidateNativeMesh(lod0Wood, "LOD0 wood", true);
            ValidateNativeMesh(lod0Foliage, "LOD0 foliage", false);
            ValidateNativeMesh(lod1Wood, "LOD1 wood", true);
            ValidateNativeMesh(lod1Foliage, "LOD1 foliage", false);

            var preparedWood = IslandGenerator.CopyGeneratedMeshData(
                lod0Wood,
                NativeWorldSizeMetres);
            var preparedFoliage = IslandGenerator.CopyGeneratedMeshData(
                lod0Foliage,
                NativeWorldSizeMetres);
            var preparedLod1Wood = IslandGenerator.CopyGeneratedMeshData(
                lod1Wood,
                NativeWorldSizeMetres);
            var preparedLod1Foliage = IslandGenerator.CopyGeneratedMeshData(
                lod1Foliage,
                NativeWorldSizeMetres);
            ValidatePreparedMesh(preparedWood, true);
            ValidatePreparedMesh(preparedFoliage, false);
            ValidatePreparedMesh(preparedLod1Wood, true);
            ValidatePreparedMesh(preparedLod1Foliage, false);
            ValidateLodPair(preparedWood, preparedLod1Wood, "wood", 16, false);
            ValidateLodPair(preparedFoliage, preparedLod1Foliage, "foliage", 4, true);
            ReleaseGeneratedMeshes();
            lod0WoodMesh = CreatePreviewMesh(preparedWood, $"Procedural Tree LOD0 Wood {seed}");
            lod1WoodMesh = CreatePreviewMesh(
                preparedLod1Wood,
                $"Procedural Tree LOD1 Wood {seed}");
            lod0FoliageMesh = CreatePreviewMesh(
                preparedFoliage,
                $"Procedural Tree LOD0 Foliage {seed}");
            lod1FoliageMesh = CreatePreviewMesh(
                preparedLod1Foliage,
                $"Procedural Tree LOD1 Foliage {seed}");
            EnsureMeshObject(WoodObjectName, out woodFilter, out woodRenderer);
            EnsureMeshObject(FoliageObjectName, out foliageFilter, out foliageRenderer);
            woodRenderer.sharedMaterial = runtimeWoodMaterial;
            foliageRenderer.sharedMaterial = runtimeFoliageMaterial;
            ApplyDisplayedLod();
            var combinedBounds = lod0WoodMesh.bounds;
            combinedBounds.Encapsulate(lod0FoliageMesh.bounds);
            FrameTree(combinedBounds);
        }
        finally
        {
            MotuNative.ReleaseMesh(ref lod0Wood);
            MotuNative.ReleaseMesh(ref lod0Foliage);
            MotuNative.ReleaseMesh(ref lod1Wood);
            MotuNative.ReleaseMesh(ref lod1Foliage);
            generating = false;
        }
    }

    [ContextMenu("Generate Next Seed")]
    public void GenerateNextSeed()
    {
        seed = unchecked(seed + 1);
        Regenerate();
    }

    public void ToggleLod()
    {
        showingLod1 = !showingLod1;
        ApplyDisplayedLod();
    }

    private void OnGUI()
    {
        GUILayout.BeginArea(ControlPanel, GUI.skin.box);
        GUILayout.Label("Procedural Tree Sandbox");
        GUILayout.Label($"Seed: {seed}");
        GUILayout.BeginHorizontal();
        GUI.enabled = orbitCamera != null;
        if (GUILayout.Button("Rotate Left"))
        {
            orbitCamera.OrbitByDegrees(-ButtonOrbitDegrees);
        }
        if (GUILayout.Button("Rotate Right"))
        {
            orbitCamera.OrbitByDegrees(ButtonOrbitDegrees);
        }
        if (GUILayout.Button("Reset View"))
        {
            orbitCamera.ResetOrientation();
        }
        GUI.enabled = !generating;
        if (GUILayout.Button("New Tree"))
        {
            GenerateNextSeed();
        }
        GUI.enabled = true;
        GUILayout.EndHorizontal();
        GUILayout.Label($"Displayed mesh: LOD {(showingLod1 ? 1 : 0)}");
        GUILayout.Label("Left drag: orbit | Right drag: pan | Wheel: zoom | M: mesh | L: LOD");
        GUILayout.EndArea();
    }

    private static Mesh CreatePreviewMesh(IslandPreparedMesh prepared, string meshName)
    {
        var mesh = IslandGenerator.CreateGeneratedMesh(prepared);
        mesh.name = meshName;
        mesh.hideFlags = HideFlags.DontSave;
        return mesh;
    }

    private void ApplyDisplayedLod()
    {
        if (woodFilter != null)
        {
            woodFilter.sharedMesh = showingLod1 ? lod1WoodMesh : lod0WoodMesh;
        }
        if (foliageFilter != null)
        {
            foliageFilter.sharedMesh = showingLod1 ? lod1FoliageMesh : lod0FoliageMesh;
        }
        if (foliageRenderer != null)
        {
            foliageRenderer.sharedMaterial = showingLod1
                ? runtimeFoliageMaterial
                : runtimeLod0FoliageMaterial;
        }
    }

    private void EnsureMeshObject(
        string objectName,
        out MeshFilter meshFilter,
        out MeshRenderer meshRenderer)
    {
        var meshTransform = transform.Find(objectName);
        if (meshTransform == null)
        {
            var meshObject = new GameObject(objectName);
            meshTransform = meshObject.transform;
            meshTransform.SetParent(transform, false);
        }
        meshFilter = meshTransform.GetComponent<MeshFilter>();
        if (meshFilter == null)
        {
            meshFilter = meshTransform.gameObject.AddComponent<MeshFilter>();
        }
        meshRenderer = meshTransform.GetComponent<MeshRenderer>();
        if (meshRenderer == null)
        {
            meshRenderer = meshTransform.gameObject.AddComponent<MeshRenderer>();
        }
    }

    private void EnsureRuntimeMaterials()
    {
        if (runtimeWoodMaterial != null
            && runtimeFoliageMaterial != null
            && runtimeLod0FoliageMaterial != null
            && surfaceNoiseTexture != null)
        {
            return;
        }
        ReleaseRuntimeMaterials();
        surfaceNoiseTexture = IslandGenerator.CreateCliffNoiseTexture();
        surfaceNoiseTexture.hideFlags = HideFlags.HideAndDontSave;
        runtimeWoodMaterial = CreateRuntimeMaterial(woodMaterial, surfaceNoiseTexture);
        runtimeFoliageMaterial = CreateRuntimeMaterial(foliageMaterial, surfaceNoiseTexture);
        runtimeFoliageMaterial.SetFloat("_CullMode", (float)CullMode.Back);
        runtimeLod0FoliageMaterial = new Material(runtimeFoliageMaterial)
        {
            name = $"{foliageMaterial.name} LOD0 Preview Runtime",
            hideFlags = HideFlags.HideAndDontSave,
        };
        runtimeLod0FoliageMaterial.SetFloat("_CullMode", (float)CullMode.Off);
        runtimeLod0FoliageMaterial.enableInstancing = true;
        if (previewCamera != null)
        {
            var cameraPosition = previewCamera.transform.position;
            runtimeFoliageMaterial.SetVector("_GrassPlayerPosition", cameraPosition);
            runtimeLod0FoliageMaterial.SetVector("_GrassPlayerPosition", cameraPosition);
        }
    }

    private static Material CreateRuntimeMaterial(Material template, Texture3D noise)
    {
        var material = new Material(template)
        {
            name = $"{template.name} Preview Runtime",
            hideFlags = HideFlags.HideAndDontSave,
        };
        material.SetTexture("_CliffNoise3D", noise);
        material.SetMatrix("_IslandWorldToLocal", Matrix4x4.identity);
        if (material.HasProperty("_WorldSize"))
        {
            material.SetFloat("_WorldSize", NativeWorldSizeMetres);
        }
        material.enableInstancing = true;
        return material;
    }

    private void ReleaseRuntimeMaterials()
    {
        DestroyPreviewObject(runtimeWoodMaterial);
        DestroyPreviewObject(runtimeFoliageMaterial);
        DestroyPreviewObject(runtimeLod0FoliageMaterial);
        DestroyPreviewObject(surfaceNoiseTexture);
        runtimeWoodMaterial = null;
        runtimeFoliageMaterial = null;
        runtimeLod0FoliageMaterial = null;
        surfaceNoiseTexture = null;
    }

    private static void DestroyPreviewObject(UnityEngine.Object value)
    {
        if (value == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(value);
        }
        else
        {
            DestroyImmediate(value);
        }
    }

    private void FrameTree(Bounds bounds)
    {
        if (previewCamera == null)
        {
            return;
        }
        var target = bounds.center;
        var distance = Mathf.Max(bounds.extents.magnitude * 2.6f, 2f);
        if (orbitCamera != null)
        {
            orbitCamera.Configure(target, distance);
        }
        else
        {
            var rotation = Quaternion.Euler(24f, 35f, 0f);
            previewCamera.transform.SetPositionAndRotation(
                target - rotation * Vector3.forward * distance,
                rotation);
        }
        previewCamera.nearClipPlane = Mathf.Max(distance * 0.002f, 0.01f);
        previewCamera.farClipPlane = Mathf.Max(distance * 5f, 20f);
    }

    private static void ValidatePreparedMesh(IslandPreparedMesh mesh, bool requireUv)
    {
        if (requireUv
            && (mesh.uv.Length != mesh.vertices.Length
                || mesh.material.Length != mesh.vertices.Length))
        {
            throw new InvalidOperationException(
                "The generated tree wood is missing its branch-local bark data.");
        }
        for (var index = 0; index < mesh.vertices.Length; index++)
        {
            if (!IsFinite(mesh.vertices[index]) || !IsFinite(mesh.normals[index]))
            {
                throw new InvalidOperationException(
                    "The generated tree contains a non-finite vertex or normal.");
            }
        }
        for (var index = 0; index < mesh.triangles.Length; index++)
        {
            if (mesh.triangles[index] < 0 || mesh.triangles[index] >= mesh.vertices.Length)
            {
                throw new InvalidOperationException(
                    "The generated tree contains an out-of-range triangle index.");
            }
        }
    }

    private static void ValidateNativeMesh(
        MotuNative.ExportMesh mesh,
        string label,
        bool requireUv)
    {
        if (mesh.handle == IntPtr.Zero
            || mesh.vertices.length <= 0
            || mesh.normals.length != mesh.vertices.length
            || mesh.triangles.length <= 0
            || mesh.triangles.length % 3 != 0
            || (requireUv && mesh.uv.length != mesh.vertices.length)
            || (requireUv && mesh.material.length != mesh.vertices.length))
        {
            throw new InvalidOperationException(
                $"The native tree generator returned an invalid {label} mesh.");
        }
    }

    private static void ValidateLodPair(
        IslandPreparedMesh lod0,
        IslandPreparedMesh lod1,
        string label,
        int triangleMultiplier,
        bool requireSharedVertices)
    {
        if (lod0.vertices.Length <= lod1.vertices.Length
            || lod0.triangles.Length != lod1.triangles.Length * triangleMultiplier)
        {
            throw new InvalidOperationException($"The tree {label} LOD topology is invalid.");
        }
        if (!requireSharedVertices)
        {
            return;
        }
        for (var vertex = 0; vertex < lod1.vertices.Length; vertex++)
        {
            if (lod1.vertices[vertex] != lod0.vertices[vertex])
            {
                throw new InvalidOperationException(
                    $"The tree {label} LOD1 vertex {vertex} does not match its LOD0 equivalent.");
            }
        }
    }

    private static bool IsFinite(Vector3 value)
    {
        return !float.IsNaN(value.x)
            && !float.IsInfinity(value.x)
            && !float.IsNaN(value.y)
            && !float.IsInfinity(value.y)
            && !float.IsNaN(value.z)
            && !float.IsInfinity(value.z);
    }

    private void ReleaseGeneratedMeshes()
    {
        if (woodFilter != null)
        {
            woodFilter.sharedMesh = null;
        }
        if (foliageFilter != null)
        {
            foliageFilter.sharedMesh = null;
        }
        ReleaseGeneratedMesh(ref lod0WoodMesh);
        ReleaseGeneratedMesh(ref lod1WoodMesh);
        ReleaseGeneratedMesh(ref lod0FoliageMesh);
        ReleaseGeneratedMesh(ref lod1FoliageMesh);
    }

    private void ReleaseGeneratedMesh(ref Mesh mesh)
    {
        if (mesh == null)
        {
            return;
        }
        if (Application.isPlaying)
        {
            Destroy(mesh);
        }
        else
        {
            DestroyImmediate(mesh);
        }
        mesh = null;
    }
}
