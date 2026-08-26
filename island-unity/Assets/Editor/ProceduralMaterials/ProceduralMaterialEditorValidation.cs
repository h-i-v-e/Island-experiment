using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;
using Debug = UnityEngine.Debug;

/// <summary>
/// Batch-friendly integration checks for the Unity document/service boundary.
/// Rust remains responsible for semantic recipe validation and generation;
/// this command exercises the real release baker, JSON schema, forms, document
/// persistence, undo, preview manifest, timings, and map classification.
/// </summary>
public static class ProceduralMaterialEditorValidation
{
    private static readonly string[] Recipes =
    {
        "Bark.json",
        "PlateBark.json",
        "cracked-stone.json",
        "rounded-river-stones.json"
    };
    private static readonly string[] SourceKinds =
    {
        "value", "fbm", "billow", "ridged", "cellular_distance", "cellular_distance_to_edge", "cellular_value"
    };

    [MenuItem("Island/Validation/Validate Procedural Material Studio")]
    public static void BatchValidateProceduralMaterialStudio()
    {
        var validationRoot = Path.Combine(ProjectRoot(), "Library", "ProceduralMaterialValidation", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(validationRoot);
        try
        {
            var baker = ReleaseBakerPath();
            var schemaEnvelope = RunBaker(baker, "schema", "--json");
            RequireSuccessfulEnvelope(schemaEnvelope, "schema");
            var schemaToken = schemaEnvelope["schema"] as JObject
                ?? throw new InvalidOperationException("The release baker returned no procedural-material schema.");
            var schema = ProceduralMaterialSchema.FromJson(schemaToken.ToString(Formatting.None));
            if (schema.Properties.Count < 100 || schema.Get("/layers/0/source/frequency") == null)
            {
                throw new InvalidOperationException("Rust schema metadata did not cover the editable layer controls.");
            }
            ValidateEditorAssets();

            foreach (var recipe in Recipes)
            {
                ValidateRecipe(baker, schema, recipe, validationRoot);
            }
            ValidatePreviewMapClassification();
            Debug.Log("Procedural Material Studio integration validation passed for all committed recipes.");
        }
        finally
        {
            if (Directory.Exists(validationRoot)) Directory.Delete(validationRoot, true);
        }
    }

    private static void ValidateRecipe(string baker, ProceduralMaterialSchema schema, string recipe, string validationRoot)
    {
        var path = Path.Combine(RecipeDirectoryAbsolutePath(), recipe);
        if (!File.Exists(path)) throw new FileNotFoundException("Committed procedural recipe is missing.", path);
        var validation = RunBaker(baker, "validate", "--recipe", path, "--json");
        RequireSuccessfulEnvelope(validation, recipe + " validation");

        var document = ProceduralMaterialDocument.Load(path);
        try
        {
            ValidateCurrentLayerShape(document, recipe);
            ValidateSchemaDrivenForms(document, schema, recipe);
            ValidateUndo(document, recipe);
            ValidateRoundTrip(document, recipe, validationRoot);
            ValidatePreview(baker, document, recipe, validationRoot);
        }
        finally
        {
            Undo.ClearUndo(document);
            UnityEngine.Object.DestroyImmediate(document);
        }
    }

    private static void ValidateEditorAssets()
    {
        var template = AssetDatabase.LoadAssetAtPath<VisualTreeAsset>("Assets/Editor/ProceduralMaterials/ProceduralMaterialEditorWindow.uxml");
        var style = AssetDatabase.LoadAssetAtPath<StyleSheet>("Assets/Editor/ProceduralMaterials/ProceduralMaterialEditorWindow.uss");
        var shader = Shader.Find("Hidden/ProceduralMaterialStudio/Preview");
        if (template == null || style == null || shader == null)
        {
            throw new InvalidOperationException("Procedural Material Studio UI or lit-preview assets could not be loaded.");
        }
        var validationMaterial = new Material(shader);
        var shaderIsComplete = shader.isSupported
            && new[] { "_MainTex", "_BumpMap", "_HeightTex", "_OcclusionTex", "_NormalGreenSign", "_LightDirection", "_LightStrength" }
                .All(validationMaterial.HasProperty);
        UnityEngine.Object.DestroyImmediate(validationMaterial);
        if (!shaderIsComplete)
        {
            throw new InvalidOperationException("Procedural Material Studio lit-preview shader is unsupported or incomplete.");
        }
        var root = template.Instantiate();
        root.styleSheets.Add(style);
        if (root.Q("layer-list-host") == null || root.Q("inspector-panel") == null || root.Q("preview-host") == null || root.Q("diagnostics-list") == null
            || root.Q("base-inspector-tab") == null || root.Q("layer-inspector-tab") == null)
        {
            throw new InvalidOperationException("Procedural Material Studio UXML is missing a required authoring region.");
        }
        var client = new RustTextureBakerClient();
        var preview = new ProceduralMaterialPreviewController(client);
        try
        {
            if (preview.Root.Query<Button>(className: "preview-tab").ToList().Count != 7)
            {
                throw new InvalidOperationException("Procedural Material Studio did not build all preview tabs.");
            }
            if (preview.Root.Q("preview-map-settings") == null || preview.Root.Q("preview-lit-settings") == null)
            {
                throw new InvalidOperationException("Procedural Material Studio did not separate contextual preview controls.");
            }
        }
        finally
        {
            preview.Dispose();
            client.Dispose();
        }
    }

    private static void ValidateCurrentLayerShape(ProceduralMaterialDocument document, string recipe)
    {
        if (!(document.Root["layers"] is JArray layers) || layers.Count == 0)
        {
            throw new InvalidOperationException(recipe + " has no current-format layers.");
        }
        foreach (var layer in layers)
        {
            if (!(layer is JObject layerObject)) throw new InvalidOperationException(recipe + " contains a non-object layer.");
            if (string.IsNullOrWhiteSpace(layerObject["id"]?.Value<string>())) throw new InvalidOperationException(recipe + " contains a layer without a stable id.");
            if (!(layerObject["outputs"] is JObject outputs)
                || !(outputs["height"] is JObject)
                || !(outputs["albedo"] is JObject albedo)
                || albedo["enabled"]?.Type != JTokenType.Boolean)
            {
                throw new InvalidOperationException(recipe + " contains a layer without current height/albedo output bindings.");
            }
        }
        _ = JObject.Parse(document.CurrentJson);
    }

    private static void ValidateSchemaDrivenForms(ProceduralMaterialDocument document, ProceduralMaterialSchema schema, string recipe)
    {
        var builder = new ProceduralMaterialFormBuilder(schema);
        var baseForm = builder.BuildBaseMaterial(document, (_, __) => { });
        if (CountAddressableControls(baseForm) < 15)
        {
            throw new InvalidOperationException(recipe + " did not populate the complete schema-driven base form.");
        }
        var layer = document.SelectedLayer ?? throw new InvalidOperationException(recipe + " did not select its first layer.");
        var originalKind = layer["source"]?["kind"]?.Value<string>() ?? "value";
        foreach (var kind in SourceKinds)
        {
            layer["source"]["kind"] = kind;
            var form = builder.BuildLayer(document, layer, 0, (_, __) => { }, _ => { });
            if (CountAddressableControls(form) < 12)
            {
                throw new InvalidOperationException(recipe + " did not populate controls for source kind " + kind + ".");
            }
        }
        layer["source"]["kind"] = originalKind;
    }

    private static int CountAddressableControls(VisualElement root)
    {
        return root.Query<VisualElement>().ToList().Count(element => element.userData is string pointer && !string.IsNullOrWhiteSpace(pointer));
    }

    private static void ValidateUndo(ProceduralMaterialDocument document, string recipe)
    {
        var originalName = document.Root["name"]?.Value<string>() ?? string.Empty;
        Undo.RegisterCompleteObjectUndo(document, "Validate procedural material undo");
        if (!document.TrySet("/name", originalName + " Undo Probe"))
        {
            throw new InvalidOperationException(recipe + " could not edit a JSON pointer.");
        }
        Undo.PerformUndo();
        document.RehydrateJson();
        if (!string.Equals(document.Root["name"]?.Value<string>(), originalName, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(recipe + " did not restore its JSON document through Unity Undo.");
        }
    }

    private static void ValidateRoundTrip(ProceduralMaterialDocument document, string recipe, string validationRoot)
    {
        var copyPath = Path.Combine(validationRoot, recipe);
        if (document.SaveCopy(copyPath) != ProceduralMaterialDocument.SaveResult.Succeeded)
        {
            throw new InvalidOperationException(recipe + " could not be saved atomically as a copy.");
        }
        var reloaded = ProceduralMaterialDocument.Load(copyPath);
        try
        {
            if (!JToken.DeepEquals(document.Root, reloaded.Root))
            {
                throw new InvalidOperationException(recipe + " changed semantically during save/reload.");
            }
            if (reloaded.Save(true) != ProceduralMaterialDocument.SaveResult.Succeeded)
            {
                throw new InvalidOperationException(recipe + " could not replace an existing recipe atomically.");
            }
            File.AppendAllText(copyPath, Environment.NewLine, new UTF8Encoding(false));
            if (!reloaded.HasExternalChanges())
            {
                throw new InvalidOperationException(recipe + " did not detect an external file change.");
            }
        }
        finally
        {
            UnityEngine.Object.DestroyImmediate(reloaded);
        }
    }

    private static void ValidatePreview(string baker, ProceduralMaterialDocument document, string recipe, string validationRoot)
    {
        var output = Path.Combine(validationRoot, Path.GetFileNameWithoutExtension(recipe) + "-preview");
        var snapshot = Path.Combine(validationRoot, Path.GetFileNameWithoutExtension(recipe) + "-snapshot.json");
        File.WriteAllText(snapshot, document.CurrentJson, new UTF8Encoding(false));
        var envelope = RunBaker(
            baker,
            "preview",
            "--recipe",
            snapshot,
            "--output",
            output,
            "--size",
            "64",
            "--normal-convention",
            "direct-x");
        RequireSuccessfulEnvelope(envelope, recipe + " preview");
        var result = new RustTextureBakerClient.BakerResult(
            RustTextureBakerClient.RequestKind.Preview,
            0,
            true,
            envelope.ToString(Formatting.None),
            string.Empty,
            envelope,
            string.Empty)
        {
            OutputDirectory = output,
        };
        if (!result.Maps.ContainsKey("Albedo")
            || !result.Maps.ContainsKey("Height")
            || !result.Maps.ContainsKey("Normal")
            || !result.Maps.ContainsKey("Occlusion")
            || !result.Maps.ContainsKey("Mask")
            || !result.Maps.ContainsKey("LayerRaw")
            || !result.Maps.ContainsKey("LayerRemapped")
            || !result.Maps.ContainsKey("LayerMask")
            || !result.Maps.ContainsKey("RawHeight")
            || !result.Maps.ContainsKey("RawHeightMetadata"))
        {
            throw new InvalidOperationException(recipe + " preview did not classify its complete map set.");
        }
        if (result.TimingsMilliseconds.Count < 2)
        {
            throw new InvalidOperationException(recipe + " preview did not report stage timings.");
        }
        var manifestPath = Path.Combine(output, "preview.manifest.json");
        var manifest = JObject.Parse(File.ReadAllText(manifestPath));
        if (manifest["complete"]?.Value<bool>() != true || !(manifest["maps"] is JArray maps) || maps.Count < 9)
        {
            throw new InvalidOperationException(recipe + " preview completion manifest was incomplete.");
        }
        if (output.StartsWith(Path.Combine(ProjectRoot(), "Assets"), StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException(recipe + " preview incorrectly entered Unity's Asset database.");
        }
        ValidateReadablePreviewTexture(output, result.Maps["Albedo"], recipe);
        ValidateRawHeightOrientation(output, result.Maps["Height"], result.Maps["RawHeight"], result.Maps["RawHeightMetadata"], recipe);
        ValidateNormalOrientation(output, result.Maps["Height"], result.Maps["Normal"], recipe);
    }

    private static void ValidateNormalOrientation(string output, string heightMapPath, string normalMapPath, string recipe)
    {
        var heightPath = Path.IsPathRooted(heightMapPath) ? heightMapPath : Path.Combine(output, heightMapPath);
        var normalPath = Path.IsPathRooted(normalMapPath) ? normalMapPath : Path.Combine(output, normalMapPath);
        var height = ProceduralMaterialPreviewController.LoadReadablePreviewTexture(heightPath);
        var normal = ProceduralMaterialPreviewController.LoadReadablePreviewTexture(normalPath);
        try
        {
            if (height == null || normal == null || height.width != normal.width || height.height != normal.height)
            {
                throw new InvalidOperationException(recipe + " height and normal maps could not be compared.");
            }
            var directScore = 0.0;
            var invertedGreenScore = 0.0;
            var samples = 0;
            var stride = Math.Max(1, height.width / 32);
            for (var y = 1; y < height.height - 1; y += stride)
            {
                for (var x = 1; x < height.width - 1; x += stride)
                {
                    var expectedX = height.GetPixel(x - 1, y).r - height.GetPixel(x + 1, y).r;
                    var expectedY = height.GetPixel(x, y - 1).r - height.GetPixel(x, y + 1).r;
                    var expectedLength = Math.Sqrt(expectedX * expectedX + expectedY * expectedY);
                    var pixel = normal.GetPixel(x, y);
                    var normalX = pixel.r * 2.0 - 1.0;
                    var normalY = pixel.g * 2.0 - 1.0;
                    var normalLength = Math.Sqrt(normalX * normalX + normalY * normalY);
                    if (expectedLength < 0.001 || normalLength < 0.001) continue;
                    directScore += (expectedX * normalX + expectedY * normalY) / (expectedLength * normalLength);
                    invertedGreenScore += (expectedX * normalX - expectedY * normalY) / (expectedLength * normalLength);
                    samples++;
                }
            }
            if (samples == 0) throw new InvalidOperationException(recipe + " normal orientation validation found no relief samples.");
            var direct = directScore / samples;
            var invertedGreen = invertedGreenScore / samples;
            if (direct < 0.9 || direct <= invertedGreen + 0.5)
            {
                throw new InvalidOperationException(recipe + " DirectX normal map did not align with Unity's height-map orientation.");
            }
        }
        finally
        {
            if (height != null) UnityEngine.Object.DestroyImmediate(height);
            if (normal != null) UnityEngine.Object.DestroyImmediate(normal);
        }
    }

    private static void ValidateRawHeightOrientation(string output, string pngMapPath, string rawMapPath, string metadataMapPath, string recipe)
    {
        var pngPath = Path.IsPathRooted(pngMapPath) ? pngMapPath : Path.Combine(output, pngMapPath);
        var rawPath = Path.IsPathRooted(rawMapPath) ? rawMapPath : Path.Combine(output, rawMapPath);
        var metadataPath = Path.IsPathRooted(metadataMapPath) ? metadataMapPath : Path.Combine(output, metadataMapPath);
        var texture = ProceduralMaterialPreviewController.LoadReadablePreviewTexture(pngPath);
        try
        {
            if (texture == null) throw new InvalidOperationException(recipe + " height PNG could not be loaded for alignment validation.");
            var metadata = JObject.Parse(File.ReadAllText(metadataPath));
            if (!string.Equals(metadata["row_order"]?.Value<string>(), "top_to_bottom", StringComparison.Ordinal))
            {
                throw new InvalidOperationException(recipe + " raw height metadata did not declare its row order.");
            }
            var raw = File.ReadAllBytes(rawPath);
            ProceduralMaterialPreviewController.FlipTopToBottomRows(raw, texture.width, texture.height, 2);
            var alignmentError = 0.0;
            var samples = 0;
            for (var y = 0; y < texture.height; y += Math.Max(1, texture.height / 16))
            {
                for (var x = 0; x < texture.width; x += Math.Max(1, texture.width / 16))
                {
                    var png = texture.GetPixel(x, y).r;
                    alignmentError += Math.Abs(png - ReadRawHeight(raw, texture.width, x, y));
                    samples++;
                }
            }
            if (alignmentError / samples > 0.005)
            {
                throw new InvalidOperationException(recipe + " raw height remained misaligned with its PNG after Unity row conversion.");
            }
        }
        finally
        {
            if (texture != null) UnityEngine.Object.DestroyImmediate(texture);
        }
    }

    private static float ReadRawHeight(byte[] raw, int width, int x, int y)
    {
        var index = (y * width + x) * 2;
        return (raw[index] | raw[index + 1] << 8) / 65535f;
    }

    private static void ValidateReadablePreviewTexture(string output, string mapPath, string recipe)
    {
        var path = Path.IsPathRooted(mapPath) ? mapPath : Path.Combine(output, mapPath);
        var texture = ProceduralMaterialPreviewController.LoadReadablePreviewTexture(path);
        try
        {
            if (texture == null || !texture.isReadable)
            {
                throw new InvalidOperationException(recipe + " preview texture was not CPU-readable for pixel and channel inspection.");
            }
            _ = texture.GetPixel(0, 0);
            if (texture.GetPixels32().Length != texture.width * texture.height)
            {
                throw new InvalidOperationException(recipe + " preview texture pixels could not be read completely.");
            }
        }
        finally
        {
            if (texture != null) UnityEngine.Object.DestroyImmediate(texture);
        }
    }

    private static void ValidatePreviewMapClassification()
    {
        var envelope = JObject.Parse(@"{
  'success': true,
  'timings_ms': { 'evaluate': 12.5, 'write': 3.5 },
  'generated_maps': [
    { 'file': 'stone_motu_unity_terrain_albedo.png' },
    { 'file': 'stone_motu_unity_terrain_height.png' },
    { 'file': 'stone_motu_unity_terrain_normal.png' },
    { 'file': 'stone_motu_unity_terrain_occlusion.png' },
    { 'file': 'stone_motu_unity_terrain_mask.png' },
    { 'file': 'stone_layer_broad_raw.png', 'kind': 'layer_raw' },
    { 'file': 'stone_preview_height.r16', 'kind': 'raw_height', 'metadata': 'stone_preview_height.json' }
  ]
}");
        var result = new RustTextureBakerClient.BakerResult(
            RustTextureBakerClient.RequestKind.Preview,
            0,
            true,
            string.Empty,
            string.Empty,
            envelope,
            string.Empty);
        if (!result.Maps.TryGetValue("Height", out var heightPath)
            || !heightPath.EndsWith("_height.png", StringComparison.OrdinalIgnoreCase)
            || !result.Maps.TryGetValue("Mask", out var maskPath)
            || !maskPath.EndsWith("_mask.png", StringComparison.OrdinalIgnoreCase)
            || !result.Maps.ContainsKey("LayerRaw")
            || !result.Maps.ContainsKey("Layer")
            || !result.Maps.ContainsKey("RawHeight")
            || !result.Maps.ContainsKey("RawHeightMetadata")
            || result.TimingsMilliseconds.Count != 2)
        {
            throw new InvalidOperationException("Preview map/timing classification did not retain the complete protocol response.");
        }
    }

    private static JObject RunBaker(string executable, params string[] arguments)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = executable,
            Arguments = string.Join(" ", arguments.Select(QuoteArgument)),
            WorkingDirectory = ProjectRoot(),
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        using (var process = Process.Start(startInfo))
        {
            if (process == null) throw new InvalidOperationException("Could not start the release procedural-material baker.");
            var standardOutput = process.StandardOutput.ReadToEnd();
            var standardError = process.StandardError.ReadToEnd();
            process.WaitForExit();
            if (process.ExitCode != 0)
            {
                throw new InvalidOperationException("The release baker exited with code " + process.ExitCode + ": " + standardError);
            }
            try
            {
                return JObject.Parse(standardOutput);
            }
            catch (JsonException error)
            {
                throw new InvalidOperationException("The release baker returned invalid JSON: " + standardOutput, error);
            }
        }
    }

    private static void RequireSuccessfulEnvelope(JObject envelope, string operation)
    {
        if (envelope?["success"]?.Value<bool>() == true) return;
        throw new InvalidOperationException(operation + " failed: " + envelope?["diagnostics"]?.ToString(Formatting.None));
    }

    private static string QuoteArgument(string value)
    {
        if (string.IsNullOrEmpty(value)) return "\"\"";
        return "\"" + value.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
    }

    private static string ReleaseBakerPath()
    {
        var filename = Application.platform == RuntimePlatform.WindowsEditor
            ? "island-texture-baker.exe"
            : "island-texture-baker";
        var path = Path.GetFullPath(Path.Combine(ProjectRoot(), "..", "island-rs", "target", "release", filename));
        if (!File.Exists(path)) throw new FileNotFoundException("Build the release procedural-material baker before Unity validation.", path);
        return path;
    }

    private static string ProjectRoot()
    {
        return Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
    }

    private static string RecipeDirectoryAbsolutePath()
    {
        return Path.GetFullPath(Path.Combine(ProjectRoot(), "..", "island-rs", "texture-recipes"));
    }
}
