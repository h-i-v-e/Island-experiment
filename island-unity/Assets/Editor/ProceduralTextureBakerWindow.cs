using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEngine;
using Debug = UnityEngine.Debug;

/// <summary>
/// Invokes the engine-neutral Rust procedural texture baker and configures the
/// imported PNGs for Unity. The procedural algorithms intentionally remain in
/// island-rs; this window owns only process invocation and asset integration.
/// </summary>
public sealed class ProceduralTextureBakerWindow : EditorWindow
{
    private const string RecipeDirectoryName = "island-rs/texture-recipes";
    private const string OutputRootAssetPath = "Assets/Generated/Textures";
    private const string BakerExecutablePreference = "Island.ProceduralTextureBaker.Executable";
    private const string UseCargoPreference = "Island.ProceduralTextureBaker.UseCargo";
    private const string LastRecipePreference = "Island.ProceduralTextureBaker.LastRecipe";
    private const string LastOutputPreference = "Island.ProceduralTextureBaker.LastOutput";
    private const string DefaultMaterialPath = "Assets/Materials/IslandTerrain.mat";

    private enum OutputProfile
    {
        Separate,
        MotuUnityTerrain,
    }

    private enum AssignmentTarget
    {
        Rock,
        Riverbed,
    }

    private string recipePath;
    private string outputAssetPath;
    private string bakerExecutable;
    private bool useCargoFallback;
    private OutputProfile profile;
    private bool force;
    private bool assignTextures;
    private Material targetMaterial;
    private AssignmentTarget assignmentTarget;
    private Vector2 scrollPosition;
    private string status;

    [MenuItem("Island/Terrain/Bake Procedural Textures")]
    private static void Open()
    {
        var window = GetWindow<ProceduralTextureBakerWindow>();
        window.titleContent = new GUIContent("Procedural Textures");
        window.minSize = new Vector2(590f, 470f);
        window.Show();
    }

    [MenuItem("Island/Terrain/Configure Generated Texture Imports")]
    public static void ConfigureCommittedGeneratedTextures()
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        var generatedRoot = Path.GetFullPath(Path.Combine(projectRoot, OutputRootAssetPath));
        if (!Directory.Exists(generatedRoot))
        {
            throw new DirectoryNotFoundException(
                $"Generated texture directory does not exist: {generatedRoot}");
        }

        AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
        var configured = 0;
        foreach (var absolutePath in Directory.EnumerateFiles(
                     generatedRoot,
                     "*.png",
                     SearchOption.AllDirectories))
        {
            var assetPath = FileUtil.GetProjectRelativePath(absolutePath).Replace('\\', '/');
            var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
            if (importer == null)
            {
                throw new InvalidOperationException(
                    $"No texture importer was created for {assetPath}.");
            }
            ConfigureImporter(importer, assetPath);
            configured++;
        }
        AssetDatabase.SaveAssets();
        Debug.Log($"Configured {configured} generated procedural texture imports.");
    }

    private void OnEnable()
    {
        recipePath = EditorPrefs.GetString(LastRecipePreference, FindDefaultRecipe());
        outputAssetPath = EditorPrefs.GetString(
            LastOutputPreference,
            $"{OutputRootAssetPath}/CrackedStone");
        bakerExecutable = EditorPrefs.GetString(BakerExecutablePreference, "island-texture-baker");
        useCargoFallback = EditorPrefs.GetBool(UseCargoPreference, false);
        profile = OutputProfile.MotuUnityTerrain;
        targetMaterial = AssetDatabase.LoadAssetAtPath<Material>(DefaultMaterialPath);
        status = "Ready.";
    }

    private void OnGUI()
    {
        scrollPosition = EditorGUILayout.BeginScrollView(scrollPosition);
        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Procedural Texture Baker", EditorStyles.boldLabel);
        EditorGUILayout.HelpBox(
            "Rust owns recipe validation and all procedural math. Unity invokes "
                + "the baker, imports its completed PNG set, and optionally assigns "
                + "the maps to the terrain material.",
            MessageType.Info);

        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Recipe and Output", EditorStyles.boldLabel);
        using (new EditorGUILayout.HorizontalScope())
        {
            recipePath = EditorGUILayout.TextField("Recipe JSON", recipePath);
            if (GUILayout.Button("Browse", GUILayout.Width(76f)))
            {
                var selected = EditorUtility.OpenFilePanel(
                    "Select Procedural Texture Recipe",
                    RecipeDirectoryAbsolutePath(),
                    "json");
                if (!string.IsNullOrEmpty(selected)) recipePath = selected;
            }
        }
        using (new EditorGUILayout.HorizontalScope())
        {
            outputAssetPath = EditorGUILayout.TextField(
                new GUIContent("Output Assets Folder", "Must be under Assets/Generated/Textures."),
                outputAssetPath);
            if (GUILayout.Button("Browse", GUILayout.Width(76f)))
            {
                var selected = EditorUtility.OpenFolderPanel(
                    "Select Generated Texture Folder",
                    OutputAssetAbsolutePath(),
                    string.Empty);
                if (!string.IsNullOrEmpty(selected))
                {
                    outputAssetPath = FileUtil.GetProjectRelativePath(selected).Replace('\\', '/');
                }
            }
        }
        profile = (OutputProfile)EditorGUILayout.EnumPopup("Output Profile", profile);
        force = EditorGUILayout.Toggle(
            new GUIContent("Replace Existing Set", "Passes --force to the safe Rust output transaction."),
            force);

        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Baker Process", EditorStyles.boldLabel);
        useCargoFallback = EditorGUILayout.Toggle(
            new GUIContent(
                "Use Cargo Fallback",
                "Runs cargo --release when no configured release executable is available."),
            useCargoFallback);
        using (new EditorGUI.DisabledScope(useCargoFallback))
        {
            bakerExecutable = EditorGUILayout.TextField(
                new GUIContent("Baker Executable", "A release island-texture-baker binary."),
                bakerExecutable);
        }
        EditorGUILayout.HelpBox(
            "Development fallback: cargo run --release --manifest-path "
                + "../island-rs/Cargo.toml --bin island-texture-baker -- ...",
            MessageType.None);

        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Optional Material Assignment", EditorStyles.boldLabel);
        assignTextures = EditorGUILayout.Toggle("Assign After Import", assignTextures);
        using (new EditorGUI.DisabledScope(!assignTextures))
        {
            targetMaterial = (Material)EditorGUILayout.ObjectField(
                "Terrain Material",
                targetMaterial,
                typeof(Material),
                false);
            assignmentTarget = (AssignmentTarget)EditorGUILayout.EnumPopup(
                "Assign As",
                assignmentTarget);
        }
        if (assignTextures)
        {
            EditorGUILayout.HelpBox(
                "Assignment is performed only after a successful bake and import. "
                    + "The existing material keeps its maps when any step fails.",
                MessageType.Warning);
        }

        EditorGUILayout.Space();
        using (new EditorGUI.DisabledScope(!CanBake()))
        {
            if (GUILayout.Button("Bake Procedural Texture Set", GUILayout.Height(34f))) Bake();
        }
        EditorGUILayout.Space();
        EditorGUILayout.LabelField("Status", status, EditorStyles.helpBox);
        EditorGUILayout.EndScrollView();
    }

    private bool CanBake()
    {
        return !string.IsNullOrWhiteSpace(recipePath)
            && File.Exists(recipePath)
            && IsOutputPathUnderGeneratedRoot(outputAssetPath)
            && (useCargoFallback || !string.IsNullOrWhiteSpace(bakerExecutable));
    }

    private void Bake()
    {
        var oldStatus = status;
        try
        {
            var recipeAbsolutePath = Path.GetFullPath(recipePath);
            var outputAbsolutePath = OutputAssetAbsolutePath();
            if (!IsOutputPathUnderGeneratedRoot(outputAssetPath))
            {
                throw new InvalidOperationException(
                    $"Output folder must be under {OutputRootAssetPath}.");
            }
            Directory.CreateDirectory(outputAbsolutePath);

            var arguments = new List<string>
            {
                "--recipe",
                recipeAbsolutePath,
                "--output",
                outputAbsolutePath,
                "--profile",
                ProfileArgument(),
            };
            if (force) arguments.Add("--force");

            EditorPrefs.SetString(LastRecipePreference, recipePath);
            EditorPrefs.SetString(LastOutputPreference, outputAssetPath);
            EditorPrefs.SetString(BakerExecutablePreference, bakerExecutable);
            EditorPrefs.SetBool(UseCargoPreference, useCargoFallback);

            status = "Running Rust baker...";
            Repaint();
            var result = RunBaker(arguments);
            if (result.ExitCode != 0)
            {
                throw new InvalidOperationException(
                    $"Baker exited with status {result.ExitCode}.\n\n"
                        + result.StandardError.Trim());
            }

            AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
            var imported = ConfigureGeneratedTextures(outputAbsolutePath);
            if (imported.Count == 0)
            {
                throw new InvalidOperationException(
                    "The baker succeeded but no PNG outputs were imported.");
            }

            if (assignTextures) AssignToMaterial(outputAssetPath, imported);
            status = "Completed.\n" + result.StandardOutput.Trim();
            Debug.Log(
                "Procedural texture bake completed.\n"
                    + result.StandardOutput.Trim(),
                this);
        }
        catch (Exception exception)
        {
            status = oldStatus;
            Debug.LogException(exception, this);
            EditorUtility.DisplayDialog(
                "Procedural Texture Bake Failed",
                exception.Message,
                "OK");
        }
    }

    private ProcessResult RunBaker(IReadOnlyList<string> arguments)
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        var startInfo = new ProcessStartInfo
        {
            FileName = useCargoFallback ? "cargo" : bakerExecutable,
            WorkingDirectory = projectRoot,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
        };
        var processArguments = new List<string>();
        if (useCargoFallback)
        {
            processArguments.Add("run");
            processArguments.Add("--release");
            processArguments.Add("--manifest-path");
            processArguments.Add(Path.Combine(projectRoot, "..", "island-rs", "Cargo.toml"));
            processArguments.Add("--bin");
            processArguments.Add("island-texture-baker");
            processArguments.Add("--");
        }
        processArguments.AddRange(arguments);
        startInfo.Arguments = JoinArguments(processArguments);

        using (var process = new Process { StartInfo = startInfo })
        {
            if (!process.Start()) throw new InvalidOperationException("Could not start the baker process.");
            var standardOutput = process.StandardOutput.ReadToEnd();
            var standardError = process.StandardError.ReadToEnd();
            process.WaitForExit();
            return new ProcessResult(process.ExitCode, standardOutput, standardError);
        }
    }

    private List<string> ConfigureGeneratedTextures(string absoluteOutputPath)
    {
        var imported = new List<string>();
        foreach (var absolutePath in Directory.EnumerateFiles(absoluteOutputPath, "*.png"))
        {
            var assetPath = FileUtil.GetProjectRelativePath(absolutePath).Replace('\\', '/');
            AssetDatabase.ImportAsset(assetPath, ImportAssetOptions.ForceSynchronousImport);
            var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
            if (importer == null)
            {
                throw new InvalidOperationException($"No texture importer was created for {assetPath}.");
            }
            ConfigureImporter(importer, assetPath);
            imported.Add(assetPath);
        }
        return imported;
    }

    private static void ConfigureImporter(TextureImporter importer, string assetPath)
    {
        var filename = Path.GetFileName(assetPath);
        var isNormal = filename.EndsWith("_normal.png", StringComparison.OrdinalIgnoreCase);
        var isAlbedo = filename.EndsWith("_albedo.png", StringComparison.OrdinalIgnoreCase);
        importer.textureType = isNormal
            ? TextureImporterType.NormalMap
            : TextureImporterType.Default;
        importer.sRGBTexture = isAlbedo;
        importer.mipmapEnabled = true;
        importer.wrapMode = TextureWrapMode.Repeat;
        importer.filterMode = FilterMode.Bilinear;
        importer.textureCompression = TextureImporterCompression.Uncompressed;
        importer.alphaSource = TextureImporterAlphaSource.None;
        importer.SaveAndReimport();
    }

    private void AssignToMaterial(string assetFolder, IReadOnlyList<string> imported)
    {
        if (targetMaterial == null)
        {
            throw new InvalidOperationException("Select a material before enabling assignment.");
        }
        var albedoPath = FindImportedMap(imported, "_albedo.png");
        var normalPath = FindImportedMap(imported, "_normal.png");
        var maskPath = FindImportedMap(imported, "_mask.png");
        if (albedoPath == null || normalPath == null || maskPath == null)
        {
            throw new InvalidOperationException(
                $"The generated set in {assetFolder} needs albedo, normal, and mask maps for assignment.");
        }

        var albedo = AssetDatabase.LoadAssetAtPath<Texture2D>(albedoPath);
        var normal = AssetDatabase.LoadAssetAtPath<Texture2D>(normalPath);
        var mask = AssetDatabase.LoadAssetAtPath<Texture2D>(maskPath);
        if (albedo == null || normal == null || mask == null)
        {
            throw new InvalidOperationException("One or more generated textures could not be loaded after import.");
        }

        var albedoProperty = assignmentTarget == AssignmentTarget.Rock
            ? "_RockAlbedoMap"
            : "_RiverBedAlbedoMap";
        var normalProperty = assignmentTarget == AssignmentTarget.Rock
            ? "_RockNormalMap"
            : "_RiverBedNormalMap";
        var maskProperty = assignmentTarget == AssignmentTarget.Rock
            ? "_RockMaskMap"
            : "_RiverBedMaskMap";
        if (!targetMaterial.HasProperty(albedoProperty)
            || !targetMaterial.HasProperty(normalProperty)
            || !targetMaterial.HasProperty(maskProperty))
        {
            throw new InvalidOperationException(
                $"Material '{targetMaterial.name}' does not expose the expected terrain texture properties.");
        }

        Undo.RecordObject(targetMaterial, "Assign procedural terrain textures");
        targetMaterial.SetTexture(albedoProperty, albedo);
        targetMaterial.SetTexture(normalProperty, normal);
        targetMaterial.SetTexture(maskProperty, mask);
        EditorUtility.SetDirty(targetMaterial);
        AssetDatabase.SaveAssets();
    }

    private static string FindImportedMap(IEnumerable<string> imported, string suffix)
    {
        return imported.FirstOrDefault(path =>
            Path.GetFileName(path).EndsWith(suffix, StringComparison.OrdinalIgnoreCase));
    }

    private string OutputAssetAbsolutePath()
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        return Path.GetFullPath(Path.Combine(projectRoot, outputAssetPath ?? string.Empty));
    }

    private static bool IsOutputPathUnderGeneratedRoot(string assetPath)
    {
        if (string.IsNullOrWhiteSpace(assetPath)) return false;
        var normalized = assetPath.Replace('\\', '/').TrimEnd('/');
        return normalized.Equals(OutputRootAssetPath, StringComparison.OrdinalIgnoreCase)
            || normalized.StartsWith(OutputRootAssetPath + "/", StringComparison.OrdinalIgnoreCase);
    }

    private static string RecipeDirectoryAbsolutePath()
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        return Path.GetFullPath(Path.Combine(projectRoot, "..", RecipeDirectoryName));
    }

    private static string FindDefaultRecipe()
    {
        var path = Path.Combine(RecipeDirectoryAbsolutePath(), "cracked-stone.json");
        return File.Exists(path) ? path : string.Empty;
    }

    private string ProfileArgument()
    {
        return profile == OutputProfile.MotuUnityTerrain
            ? "motu_unity_terrain"
            : "separate";
    }

    private static string JoinArguments(IEnumerable<string> arguments)
    {
        var builder = new StringBuilder();
        foreach (var argument in arguments)
        {
            if (builder.Length > 0) builder.Append(' ');
            builder.Append(QuoteArgument(argument));
        }
        return builder.ToString();
    }

    private static string QuoteArgument(string argument)
    {
        if (argument.Length > 0 && argument.All(character =>
                !char.IsWhiteSpace(character)
                && character != '\"'
                && character != '\\'))
        {
            return argument;
        }
        return "\"" + argument.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
    }

    private readonly struct ProcessResult
    {
        internal ProcessResult(int exitCode, string standardOutput, string standardError)
        {
            ExitCode = exitCode;
            StandardOutput = standardOutput;
            StandardError = standardError;
        }

        internal int ExitCode { get; }
        internal string StandardOutput { get; }
        internal string StandardError { get; }
    }
}
