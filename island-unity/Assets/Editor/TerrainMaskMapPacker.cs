using System;
using System.IO;
using UnityEditor;
using UnityEngine;

public sealed class TerrainMaskMapPacker : EditorWindow
{
    private enum MaterialTarget
    {
        RockAndCliff,
        Riverbed,
    }

    private Texture2D heightMap;
    private bool invertHeight;
    private Texture2D occlusionMap;
    private bool invertOcclusion;
    private Material assignToMaterial;
    private MaterialTarget materialTarget;

    [MenuItem("Island/Terrain/Pack Height + Occlusion Mask")]
    private static void Open()
    {
        var window = GetWindow<TerrainMaskMapPacker>();
        window.titleContent = new GUIContent("Terrain Mask Packer");
        window.minSize = new Vector2(420f, 310f);
        window.Show();
    }

    private void OnGUI()
    {
        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Terrain Mask Map", EditorStyles.boldLabel);
        EditorGUILayout.HelpBox(
            "Packs grayscale source textures into the format used by the "
                + "island terrain shader: R = height, G = occlusion, "
                + "B = 0, A = 1. The saved texture is imported as linear data.",
            MessageType.Info);

        EditorGUILayout.Space();
        heightMap = (Texture2D)EditorGUILayout.ObjectField(
            new GUIContent("Height Map (Red)", "A missing height map uses neutral 50% gray."),
            heightMap,
            typeof(Texture2D),
            false);
        using (new EditorGUI.DisabledScope(heightMap == null))
        {
            invertHeight = EditorGUILayout.Toggle("Invert Height", invertHeight);
        }

        EditorGUILayout.Space();
        occlusionMap = (Texture2D)EditorGUILayout.ObjectField(
            new GUIContent("Occlusion Map (Green)", "A missing occlusion map uses white (no occlusion)."),
            occlusionMap,
            typeof(Texture2D),
            false);
        using (new EditorGUI.DisabledScope(occlusionMap == null))
        {
            invertOcclusion = EditorGUILayout.Toggle("Invert Occlusion", invertOcclusion);
        }

        if (heightMap != null
            && occlusionMap != null
            && (heightMap.width != occlusionMap.width
                || heightMap.height != occlusionMap.height))
        {
            EditorGUILayout.HelpBox(
                "The source dimensions differ. Both maps will be resampled to "
                    + "the largest source width and height.",
                MessageType.Warning);
        }

        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Optional Material Assignment", EditorStyles.boldLabel);
        assignToMaterial = (Material)EditorGUILayout.ObjectField(
            "Terrain Material",
            assignToMaterial,
            typeof(Material),
            false);
        using (new EditorGUI.DisabledScope(assignToMaterial == null))
        {
            materialTarget = (MaterialTarget)EditorGUILayout.EnumPopup(
                "Assign As",
                materialTarget);
        }

        GUILayout.FlexibleSpace();
        using (new EditorGUI.DisabledScope(heightMap == null && occlusionMap == null))
        {
            if (GUILayout.Button("Pack and Save PNG", GUILayout.Height(32f)))
            {
                PackAndSave();
            }
        }
    }

    private void PackAndSave()
    {
        var suggestedName = materialTarget == MaterialTarget.RockAndCliff
            ? "RockMaskMap.png"
            : "RiverbedMaskMap.png";
        var assetPath = EditorUtility.SaveFilePanelInProject(
            "Save Terrain Mask Map",
            suggestedName,
            "png",
            "Choose a location inside this project's Assets folder.");
        if (string.IsNullOrEmpty(assetPath))
        {
            return;
        }

        try
        {
            var packed = TerrainMaskMapUtility.Pack(
                heightMap,
                invertHeight,
                occlusionMap,
                invertOcclusion);
            var absolutePath = Path.GetFullPath(Path.Combine(
                Directory.GetCurrentDirectory(),
                assetPath));
            File.WriteAllBytes(absolutePath, packed.EncodeToPNG());
            DestroyImmediate(packed);

            AssetDatabase.ImportAsset(
                assetPath,
                ImportAssetOptions.ForceSynchronousImport);
            TerrainMaskMapUtility.ConfigureOutputImporter(assetPath);
            var output = AssetDatabase.LoadAssetAtPath<Texture2D>(assetPath);

            if (assignToMaterial != null)
            {
                var propertyName = materialTarget == MaterialTarget.RockAndCliff
                    ? "_RockMaskMap"
                    : "_RiverBedMaskMap";
                if (!assignToMaterial.HasProperty(propertyName))
                {
                    throw new InvalidOperationException(
                        $"Material '{assignToMaterial.name}' does not have "
                            + $"the terrain shader property {propertyName}.");
                }

                Undo.RecordObject(assignToMaterial, "Assign terrain mask map");
                assignToMaterial.SetTexture(propertyName, output);
                EditorUtility.SetDirty(assignToMaterial);
                AssetDatabase.SaveAssets();
            }

            Selection.activeObject = output;
            EditorGUIUtility.PingObject(output);
            Debug.Log(
                $"Packed terrain mask map at {assetPath}: "
                    + $"R = height, G = occlusion ({output.width}x{output.height}).",
                output);
        }
        catch (Exception exception)
        {
            Debug.LogException(exception);
            EditorUtility.DisplayDialog(
                "Terrain Mask Packing Failed",
                exception.Message,
                "OK");
        }
    }
}

internal static class TerrainMaskMapUtility
{
    private const float NeutralHeight = 0.5f;
    private const float Unoccluded = 1f;

    internal static Texture2D Pack(
        Texture2D heightMap,
        bool invertHeight,
        Texture2D occlusionMap,
        bool invertOcclusion)
    {
        if (heightMap == null && occlusionMap == null)
        {
            throw new ArgumentException(
                "Select at least one height or occlusion source texture.");
        }

        var width = Mathf.Max(
            heightMap != null ? heightMap.width : 1,
            occlusionMap != null ? occlusionMap.width : 1);
        var height = Mathf.Max(
            heightMap != null ? heightMap.height : 1,
            occlusionMap != null ? occlusionMap.height : 1);
        var heights = ReadRedChannel(
            heightMap,
            width,
            height,
            NeutralHeight,
            invertHeight);
        var occlusion = ReadRedChannel(
            occlusionMap,
            width,
            height,
            Unoccluded,
            invertOcclusion);
        var pixels = new Color32[checked(width * height)];

        for (var index = 0; index < pixels.Length; index++)
        {
            pixels[index] = new Color(
                heights[index],
                occlusion[index],
                0f,
                1f);
        }

        var output = new Texture2D(
            width,
            height,
            TextureFormat.RGBA32,
            false,
            true)
        {
            name = "Packed Terrain Mask Map",
            wrapMode = TextureWrapMode.Repeat,
            filterMode = FilterMode.Bilinear,
        };
        output.SetPixels32(pixels);
        output.Apply(false, false);
        return output;
    }

    internal static void ConfigureOutputImporter(string assetPath)
    {
        var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
        if (importer == null)
        {
            throw new InvalidOperationException(
                $"Unity did not create a texture importer for {assetPath}.");
        }

        importer.textureType = TextureImporterType.Default;
        importer.sRGBTexture = false;
        importer.alphaSource = TextureImporterAlphaSource.None;
        importer.mipmapEnabled = true;
        importer.wrapMode = TextureWrapMode.Repeat;
        importer.filterMode = FilterMode.Bilinear;
        importer.textureCompression = TextureImporterCompression.Uncompressed;
        importer.SaveAndReimport();
    }

    private static float[] ReadRedChannel(
        Texture2D source,
        int targetWidth,
        int targetHeight,
        float missingValue,
        bool invert)
    {
        if (source == null)
        {
            var missing = new float[checked(targetWidth * targetHeight)];
            Array.Fill(missing, invert ? 1f - missingValue : missingValue);
            return missing;
        }

        var assetPath = AssetDatabase.GetAssetPath(source);
        var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
        if (importer == null)
        {
            if (!source.isReadable)
            {
                throw new InvalidOperationException(
                    $"Texture '{source.name}' is not a readable imported texture.");
            }
            return ResampleRed(source, targetWidth, targetHeight, invert);
        }

        var wasReadable = importer.isReadable;
        var wasSrgb = importer.sRGBTexture;
        var previousCompression = importer.textureCompression;
        var needsTemporaryReimport = !wasReadable
            || wasSrgb
            || previousCompression != TextureImporterCompression.Uncompressed;

        try
        {
            if (needsTemporaryReimport)
            {
                importer.isReadable = true;
                importer.sRGBTexture = false;
                importer.textureCompression = TextureImporterCompression.Uncompressed;
                importer.SaveAndReimport();
            }
            var readableSource = needsTemporaryReimport
                ? AssetDatabase.LoadAssetAtPath<Texture2D>(assetPath)
                : source;
            if (readableSource == null)
            {
                throw new InvalidOperationException(
                    $"Unity could not reload texture '{source.name}' after changing "
                        + "its temporary import settings.");
            }
            return ResampleRed(
                readableSource,
                targetWidth,
                targetHeight,
                invert);
        }
        finally
        {
            if (needsTemporaryReimport)
            {
                importer.isReadable = wasReadable;
                importer.sRGBTexture = wasSrgb;
                importer.textureCompression = previousCompression;
                importer.SaveAndReimport();
            }
        }
    }

    private static float[] ResampleRed(
        Texture2D source,
        int targetWidth,
        int targetHeight,
        bool invert)
    {
        var values = new float[checked(targetWidth * targetHeight)];
        for (var y = 0; y < targetHeight; y++)
        {
            var v = (y + 0.5f) / targetHeight;
            for (var x = 0; x < targetWidth; x++)
            {
                var u = (x + 0.5f) / targetWidth;
                var value = source.GetPixelBilinear(u, v).r;
                values[y * targetWidth + x] = invert ? 1f - value : value;
            }
        }
        return values;
    }
}
