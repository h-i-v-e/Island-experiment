using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEditor;
using UnityEditor.UIElements;
using UnityEngine;
using UnityEngine.UIElements;

/// <summary>
/// JSON-backed Procedural Material Studio. Unity owns authoring state and
/// asset integration; Rust owns schema validation, noise evaluation and map
/// generation.
/// </summary>
public sealed class ProceduralMaterialEditorWindow : EditorWindow
{
    private const string UxmlPath = "Assets/Editor/ProceduralMaterials/ProceduralMaterialEditorWindow.uxml";
    private const string UssPath = "Assets/Editor/ProceduralMaterials/ProceduralMaterialEditorWindow.uss";
    private const string LastRecipePreference = "Island.ProceduralMaterialStudio.LastRecipe";
    private const string OutputRootAssetPath = "Assets/Generated/Textures";
    private const string RecipeDirectoryName = "island-rs/texture-recipes";
    private const string DefaultMaterialPath = "Assets/Materials/IslandTerrain.mat";

    private ProceduralMaterialDocument document;
    private ProceduralMaterialSchema schema;
    private ProceduralMaterialFormBuilder formBuilder;
    private RustTextureBakerClient bakerClient;
    private ProceduralMaterialPreviewController previewController;
    private ProceduralMaterialLayerList layerList;

    private VisualElement baseInspector;
    private VisualElement layerInspector;
    private VisualElement layerListHost;
    private ScrollView inspectorPanel;
    private ToolbarButton baseInspectorTab;
    private ToolbarButton layerInspectorTab;
    private Label documentLabel;
    private Label statusLabel;
    private ScrollView diagnosticsList;
    private TextField outputField;
    private DropdownField profileField;
    private Toggle replaceToggle;
    private Toggle assignToggle;
    private ObjectField materialField;
    private DropdownField assignmentTarget;
    private TextField bakerExecutable;
    private Toggle cargoToggle;
    private bool closing;
    private bool closePromptPending;
    private bool showLayerInspector = true;

    [MenuItem("Island/Terrain/Procedural Material Studio")]
    public static void OpenWindow()
    {
        var window = GetWindow<ProceduralMaterialEditorWindow>();
        window.titleContent = new GUIContent("Procedural Material Studio");
        window.minSize = new Vector2(980f, 620f);
        window.Show();
    }

    private void OnEnable()
    {
        titleContent = new GUIContent("Procedural Material Studio");
        minSize = new Vector2(980f, 620f);
        closing = false;
        if (bakerClient == null) bakerClient = new RustTextureBakerClient();
        schema = ProceduralMaterialSchema.CreateFallback();
        formBuilder = new ProceduralMaterialFormBuilder(schema);
        Undo.undoRedoPerformed -= OnUndoRedo;
        Undo.undoRedoPerformed += OnUndoRedo;
    }

    private void CreateGUI()
    {
        closing = false;
        rootVisualElement.Clear();
        var template = AssetDatabase.LoadAssetAtPath<VisualTreeAsset>(UxmlPath);
        if (template != null) template.CloneTree(rootVisualElement);
        else BuildFallbackUi(rootVisualElement);
        var styleSheet = AssetDatabase.LoadAssetAtPath<StyleSheet>(UssPath);
        if (styleSheet != null) rootVisualElement.styleSheets.Add(styleSheet);

        WireUi();
        previewController?.Dispose();
        previewController = new ProceduralMaterialPreviewController(bakerClient);
        rootVisualElement.Q<VisualElement>("preview-host")?.Add(previewController.Root);
        LoadInitialDocument();
        RequestSchemaMetadata();
    }

    private void OnDisable()
    {
        if (closePromptPending) return;
        if (!closing && !closePromptPending && document != null && document.IsDirty)
        {
            closePromptPending = true;
            // Re-show immediately so an asynchronous Save validation can finish
            // before Unity destroys the disabled window instance.
            Show();
            Focus();
            ConfirmDiscardIfNeeded(confirmed =>
            {
                closePromptPending = false;
                if (confirmed)
                {
                    closing = true;
                    DisposeWindowResources();
                    EditorApplication.delayCall += () =>
                    {
                        if (this != null) Close();
                    };
                }
                else
                {
                    closing = false;
                    ReopenAfterCancelledClose();
                }
            });
            return;
        }

        closing = true;
        DisposeWindowResources();
    }

    private void DisposeWindowResources()
    {
        Undo.undoRedoPerformed -= OnUndoRedo;
        if (document != null) document.Changed -= OnDocumentChanged;
        previewController?.Dispose();
        previewController = null;
        bakerClient?.Dispose();
        bakerClient = null;
        if (document != null) DestroyImmediate(document);
        document = null;
    }

    private void ReopenAfterCancelledClose()
    {
        EditorApplication.delayCall += () =>
        {
            if (this == null || closing) return;
            Show();
            Focus();
        };
    }

    private void WireUi()
    {
        rootVisualElement.RegisterCallback<KeyDownEvent>(OnShortcut, TrickleDown.TrickleDown);
        rootVisualElement.Q<ToolbarButton>("new-button")?.RegisterCallback<ClickEvent>(_ => NewDocument());
        rootVisualElement.Q<ToolbarButton>("open-button")?.RegisterCallback<ClickEvent>(_ => OpenDocument());
        rootVisualElement.Q<ToolbarButton>("save-button")?.RegisterCallback<ClickEvent>(_ => SaveDocument());
        rootVisualElement.Q<ToolbarButton>("save-as-button")?.RegisterCallback<ClickEvent>(_ => SaveAsDocument());
        rootVisualElement.Q<ToolbarButton>("duplicate-button")?.RegisterCallback<ClickEvent>(_ => DuplicateDocument());
        rootVisualElement.Q<ToolbarButton>("revert-button")?.RegisterCallback<ClickEvent>(_ => RevertDocument());
        rootVisualElement.Q<ToolbarButton>("validate-button")?.RegisterCallback<ClickEvent>(_ => ValidateDocument());
        rootVisualElement.Q<ToolbarButton>("preview-button")?.RegisterCallback<ClickEvent>(_ => PreviewDocument());
        rootVisualElement.Q<ToolbarButton>("bake-button")?.RegisterCallback<ClickEvent>(_ => BakeDocument());
        rootVisualElement.Q<Button>("add-layer-button")?.RegisterCallback<ClickEvent>(_ => AddLayer());
        rootVisualElement.Q<Button>("duplicate-layer-button")?.RegisterCallback<ClickEvent>(_ => DuplicateLayer());
        rootVisualElement.Q<Button>("delete-layer-button")?.RegisterCallback<ClickEvent>(_ => DeleteLayer());
        rootVisualElement.Q<Button>("solo-layer-button")?.RegisterCallback<ClickEvent>(_ => previewController?.ToggleSoloLayer());
        rootVisualElement.Q<Button>("compare-layer-button")?.RegisterCallback<ClickEvent>(_ => previewController?.ToggleBeforeAfter());

        baseInspector = rootVisualElement.Q<VisualElement>("base-inspector");
        layerInspector = rootVisualElement.Q<VisualElement>("layer-inspector");
        layerListHost = rootVisualElement.Q<VisualElement>("layer-list-host");
        inspectorPanel = rootVisualElement.Q<ScrollView>("inspector-panel");
        baseInspectorTab = rootVisualElement.Q<ToolbarButton>("base-inspector-tab");
        layerInspectorTab = rootVisualElement.Q<ToolbarButton>("layer-inspector-tab");
        baseInspectorTab?.RegisterCallback<ClickEvent>(_ => SetInspectorMode(false));
        layerInspectorTab?.RegisterCallback<ClickEvent>(_ => SetInspectorMode(true));
        documentLabel = rootVisualElement.Q<Label>("document-label");
        statusLabel = rootVisualElement.Q<Label>("status-label");
        diagnosticsList = rootVisualElement.Q<ScrollView>("diagnostics-list");
        outputField = rootVisualElement.Q<TextField>("output-field");
        profileField = rootVisualElement.Q<DropdownField>("profile-field");
        replaceToggle = rootVisualElement.Q<Toggle>("replace-toggle");
        assignToggle = rootVisualElement.Q<Toggle>("assign-toggle");
        materialField = rootVisualElement.Q<ObjectField>("material-field");
        assignmentTarget = rootVisualElement.Q<DropdownField>("assignment-target");
        bakerExecutable = rootVisualElement.Q<TextField>("baker-executable");
        cargoToggle = rootVisualElement.Q<Toggle>("cargo-toggle");

        if (profileField != null)
        {
            profileField.choices = new List<string> { "separate", "motu_unity_terrain" };
            profileField.value = "motu_unity_terrain";
        }
        if (assignmentTarget != null)
        {
            assignmentTarget.choices = new List<string> { "Rock", "Riverbed" };
            assignmentTarget.value = "Rock";
        }
        if (outputField != null) outputField.value = OutputRootAssetPath + "/CrackedStone";
        if (materialField != null) materialField.value = AssetDatabase.LoadAssetAtPath<Material>(DefaultMaterialPath);
        if (bakerExecutable != null)
        {
            bakerExecutable.value = bakerClient.ConfiguredExecutable;
            bakerExecutable.RegisterValueChangedCallback(change => bakerClient.SetConfiguredExecutable(change.newValue));
        }
        if (cargoToggle != null)
        {
            cargoToggle.value = bakerClient.UseCargoFallback;
            cargoToggle.RegisterValueChangedCallback(change => bakerClient.SetUseCargoFallback(change.newValue));
        }
    }

    private void OnShortcut(KeyDownEvent eventData)
    {
        if (eventData == null) return;
        if (eventData.keyCode == KeyCode.F5)
        {
            PreviewDocument();
            eventData.StopImmediatePropagation();
            return;
        }
        if (!eventData.actionKey) return;
        switch (eventData.keyCode)
        {
            case KeyCode.N:
                NewDocument();
                break;
            case KeyCode.O:
                OpenDocument();
                break;
            case KeyCode.S when eventData.shiftKey:
                SaveAsDocument();
                break;
            case KeyCode.S:
                SaveDocument();
                break;
            case KeyCode.Return:
            case KeyCode.KeypadEnter:
                PreviewDocument();
                break;
            default:
                return;
        }
        eventData.StopImmediatePropagation();
    }

    private void LoadInitialDocument()
    {
        if (document != null)
        {
            document.Changed -= OnDocumentChanged;
            document.Changed += OnDocumentChanged;
            RebuildEditor();
            return;
        }
        var path = EditorPrefs.GetString(LastRecipePreference, string.Empty);
        if (string.IsNullOrWhiteSpace(path) || !File.Exists(path)) path = FindDefaultRecipe();
        if (!string.IsNullOrWhiteSpace(path) && File.Exists(path))
        {
            try
            {
                SetDocument(ProceduralMaterialDocument.Load(path));
                SetStatus("Opened " + Path.GetFileName(path) + ".");
                return;
            }
            catch (Exception exception)
            {
                Debug.LogException(exception);
                SetStatus("Could not open the last recipe; a new document was created.");
            }
        }
        SetDocument(ProceduralMaterialDocument.CreateNew());
        SetStatus("New unsaved recipe.");
    }

    private void SetDocument(ProceduralMaterialDocument value, Action<bool> completed = null)
    {
        var next = value ?? ProceduralMaterialDocument.CreateNew();
        if (document != null && document.IsDirty)
        {
            ConfirmDiscardIfNeeded(confirmed =>
            {
                if (!confirmed)
                {
                    DestroyImmediate(next);
                    completed?.Invoke(false);
                    return;
                }
                SetDocumentNow(next);
                completed?.Invoke(true);
            });
            return;
        }
        SetDocumentNow(next);
        completed?.Invoke(true);
    }

    private void SetDocumentNow(ProceduralMaterialDocument value)
    {
        if (document != null) document.Changed -= OnDocumentChanged;
        if (document != null) DestroyImmediate(document);
        document = value ?? ProceduralMaterialDocument.CreateNew();
        document.Changed += OnDocumentChanged;
        previewController?.SetDocument(document);
        RebuildEditor();
    }

    private void RequestSchemaMetadata()
    {
        bakerClient.RequestSchema(result =>
        {
            if (closing || !result.Succeeded) return;
            var schemaJson = result.Envelope?["schema"]?.ToString(Formatting.None)
                ?? result.Envelope?["json_schema"]?.ToString(Formatting.None)
                ?? result.Envelope?["data"]?["schema"]?.ToString(Formatting.None)
                ?? result.StandardOutput;
            if (string.IsNullOrWhiteSpace(schemaJson)) return;
            schema = ProceduralMaterialSchema.FromJson(schemaJson);
            formBuilder = new ProceduralMaterialFormBuilder(schema);
            document?.SetSchemaMetadata(schemaJson);
            RebuildEditor();
        });
    }

    private void RebuildEditor()
    {
        if (document == null || baseInspector == null) return;
        baseInspector.Clear();
        layerInspector.Clear();
        baseInspector.Add(formBuilder.BuildBaseMaterial(document, ApplyJsonEdit));
        var selected = document.SelectedLayer;
        var selectedIndex = FindLayerIndex(selected);
        layerInspector.Add(formBuilder.BuildLayer(document, selected, selectedIndex, ApplyJsonEdit, SelectLayer));
        RefreshInspectorMode();

        if (layerListHost != null)
        {
            layerListHost.Clear();
            layerList = new ProceduralMaterialLayerList(document, SelectLayer, ReorderLayer, RenameLayer, EnableLayer);
            layerListHost.Add(layerList.Root);
        }
        documentLabel.text = (document.IsDirty ? "● " : string.Empty)
            + (string.IsNullOrWhiteSpace(document.SourceFilePath) ? "Unsaved recipe" : document.SourceFilePath)
            + "  •  " + document.CurrentHash.Substring(0, Math.Min(12, document.CurrentHash.Length));
        if (!string.IsNullOrWhiteSpace(document.LastBakeHash))
        {
            documentLabel.text += string.Equals(document.LastBakeHash, document.CurrentHash, StringComparison.OrdinalIgnoreCase)
                ? "  •  Last bake matches"
                : "  •  Changed since last bake";
        }
    }

    private void OnDocumentChanged(ProceduralMaterialDocument changed)
    {
        if (changed != document) return;
        RebuildEditor();
        previewController?.NotifyDocumentChanged();
    }

    private void OnUndoRedo()
    {
        if (document == null) return;
        document.RehydrateJson();
        RebuildEditor();
        previewController?.NotifyDocumentChanged();
    }

    private void ApplyJsonEdit(string pointer, JToken value)
    {
        if (document == null) return;
        Undo.RecordObject(document, "Edit procedural material");
        if (!document.TrySet(pointer, value)) return;
        var kind = value?.Type == JTokenType.String ? value.Value<string>() : null;
        if (pointer.EndsWith("/mask/kind", StringComparison.Ordinal))
        {
            ConfigureMaskShapeFromKind(pointer, kind);
        }
        else if (string.Equals(pointer, "/material/kind", StringComparison.Ordinal))
        {
            ConfigureMaterialShapeFromKind(kind);
        }
        else if (pointer.EndsWith("/outputs/height/blend/kind", StringComparison.Ordinal))
        {
            ConfigureHeightBlendShapeFromKind(pointer, kind);
        }
        else if (pointer.EndsWith("/outputs/albedo/colour_map/kind", StringComparison.Ordinal))
        {
            ConfigureColourMapShapeFromKind(pointer, kind);
        }
        else if (string.Equals(pointer, "/occlusion/combine/kind", StringComparison.Ordinal))
        {
            ConfigureOcclusionCombineShapeFromKind(kind);
        }
    }

    private void ConfigureMaterialShapeFromKind(string kind)
    {
        JObject material;
        switch (kind)
        {
            case "layered_noise":
                material = new JObject
                {
                    ["kind"] = kind,
                    ["frequency"] = 1.0,
                    ["amplitude"] = 1.0,
                    ["octaves"] = 4,
                    ["lacunarity"] = 2.0,
                    ["gain"] = 0.5,
                    ["offset"] = 0.0,
                };
                break;
            case "rounded_stones":
                material = new JObject
                {
                    ["kind"] = kind,
                    ["cells_x"] = 14,
                    ["cells_y"] = 14,
                    ["stone_radius"] = 0.36,
                    ["cell_jitter"] = 0.23,
                    ["warp_amplitude"] = 0.08,
                    ["anisotropy"] = 1.0,
                    ["stone_height"] = 0.12,
                    ["stone_variation"] = 0.045,
                    ["gap_height"] = -0.012,
                    ["sand_amplitude"] = 0.009,
                    ["edge_softness"] = 0.08,
                };
                break;
            default:
                material = new JObject
                {
                    ["kind"] = "cracked_stone",
                    ["cells_x"] = 8,
                    ["cells_y"] = 8,
                    ["cell_jitter"] = 0.25,
                    ["warp_amplitude"] = 0.16,
                    ["crack_width"] = 0.035,
                    ["shoulder_width"] = 0.18,
                    ["crack_depth"] = 0.13,
                    ["slab_variation"] = 0.035,
                    ["fracture_probability"] = 0.28,
                    ["fracture_depth"] = 0.045,
                    ["surface_amplitude"] = 0.014,
                    ["broad_variation"] = 0.018,
                };
                break;
        }
        document.TrySet("/material", material);
    }

    private void ConfigureHeightBlendShapeFromKind(string kindPointer, string kind)
    {
        var blend = ReadTokenAtPointer(document.Root, ParentPointer(kindPointer)) as JObject;
        if (blend == null) return;
        blend.Remove("amount");
        if (string.Equals(kind, "lerp", StringComparison.Ordinal)) blend["amount"] = 0.5;
        document.SetJson(document.Root.ToString(Formatting.None));
    }

    private void ConfigureColourMapShapeFromKind(string kindPointer, string kind)
    {
        var colourMap = ReadTokenAtPointer(document.Root, ParentPointer(kindPointer)) as JObject;
        if (colourMap == null) return;
        colourMap.Remove("first");
        colourMap.Remove("second");
        colourMap.Remove("stops");
        if (string.Equals(kind, "ramp", StringComparison.Ordinal))
        {
            colourMap["first"] = new JArray(0.25, 0.27, 0.24);
            colourMap["second"] = new JArray(0.42, 0.36, 0.28);
        }
        else
        {
            colourMap["stops"] = new JArray
            {
                new JObject { ["position"] = 0.0, ["colour"] = new JArray(0.25, 0.27, 0.24) },
                new JObject { ["position"] = 1.0, ["colour"] = new JArray(0.42, 0.36, 0.28) },
            };
        }
        document.SetJson(document.Root.ToString(Formatting.None));
    }

    private void ConfigureOcclusionCombineShapeFromKind(string kind)
    {
        var combine = ReadTokenAtPointer(document.Root, "/occlusion/combine") as JObject;
        if (combine == null) return;
        combine.Remove("cavity_weight");
        combine.Remove("horizon_weight");
        if (string.Equals(kind, "weighted_minimum", StringComparison.Ordinal))
        {
            combine["cavity_weight"] = 0.5;
            combine["horizon_weight"] = 0.5;
        }
        document.SetJson(document.Root.ToString(Formatting.None));
    }

    private static string ParentPointer(string pointer)
    {
        var separator = pointer.LastIndexOf('/');
        return separator <= 0 ? string.Empty : pointer.Substring(0, separator);
    }

    private static JToken ReadTokenAtPointer(JToken root, string pointer)
    {
        if (root == null || string.IsNullOrWhiteSpace(pointer) || pointer[0] != '/') return null;
        var current = root;
        foreach (var rawSegment in pointer.Substring(1).Split('/'))
        {
            var segment = rawSegment.Replace("~1", "/").Replace("~0", "~");
            if (current is JObject objectCurrent)
            {
                current = objectCurrent[segment];
            }
            else if (current is JArray arrayCurrent && int.TryParse(segment, out var index) && index >= 0 && index < arrayCurrent.Count)
            {
                current = arrayCurrent[index];
            }
            else
            {
                return null;
            }
        }
        return current;
    }

    private void ConfigureMaskShapeFromKind(string kindPointer, string kind)
    {
        var segments = kindPointer.Trim('/').Split('/');
        if (segments.Length != 4 || !string.Equals(segments[0], "layers", StringComparison.Ordinal) || !int.TryParse(segments[1], out var index)) return;
        var layers = document.Layers;
        if (layers == null || index < 0 || index >= layers.Count || !(layers[index] is JObject layer)) return;
        var mask = layer["mask"] as JObject;
        if (mask == null) return;
        mask.Remove("source");
        mask.Remove("layer_id");
        mask.Remove("remap");
        if (string.Equals(kind, "noise", StringComparison.Ordinal))
        {
            mask["source"] = new JObject
            {
                ["kind"] = "value",
                ["frequency"] = 4,
                ["octaves"] = 2,
                ["lacunarity"] = 2.0,
                ["gain"] = 0.5,
                ["offset"] = new JArray(0.0, 0.0),
                ["seed_domain"] = 401,
                ["domain_warp"] = null,
                ["cellular_jitter"] = 0.25,
            };
            mask["remap"] = DefaultRemap();
        }
        else if (string.Equals(kind, "layer", StringComparison.Ordinal))
        {
            var earlier = index > 0 ? document.GetLayerId(layers[index - 1] as JObject, index - 1) : string.Empty;
            mask["layer_id"] = earlier;
            mask["remap"] = DefaultRemap();
        }
        document.SetJson(document.Root.ToString(Formatting.None));
    }

    private static JObject DefaultRemap()
    {
        return new JObject
        {
            ["input_min"] = 0.0,
            ["input_max"] = 1.0,
            ["invert"] = false,
            ["contrast"] = 1.0,
            ["bias"] = 0.0,
            ["clamp"] = true,
        };
    }

    private void SelectLayer(string layerId)
    {
        if (document == null) return;
        showLayerInspector = true;
        if (!string.Equals(document.SelectedLayerId, layerId, StringComparison.Ordinal)) document.SelectLayer(layerId);
        else RefreshInspectorMode();
    }

    private void SetInspectorMode(bool layer)
    {
        showLayerInspector = layer;
        RefreshInspectorMode();
    }

    private void RefreshInspectorMode()
    {
        if (baseInspector != null) baseInspector.style.display = showLayerInspector ? DisplayStyle.None : DisplayStyle.Flex;
        if (layerInspector != null) layerInspector.style.display = showLayerInspector ? DisplayStyle.Flex : DisplayStyle.None;
        baseInspectorTab?.EnableInClassList("inspector-tab-active", !showLayerInspector);
        layerInspectorTab?.EnableInClassList("inspector-tab-active", showLayerInspector);
        if (layerInspectorTab != null)
        {
            var selected = document?.SelectedLayer;
            var selectedIndex = FindLayerIndex(selected);
            layerInspectorTab.text = "Selected layer";
            layerInspectorTab.tooltip = selected == null
                ? "Select a layer from the stack to edit it."
                : "Editing " + document.GetLayerName(selected, selectedIndex) + ": source, remap, mask, height and albedo outputs.";
        }
    }

    private void RenameLayer(string layerId, string value)
    {
        var index = FindLayerIndex(layerId);
        if (index < 0) return;
        ApplyJsonEdit(LayerPointer(index) + "/name", value);
    }

    private void EnableLayer(string layerId, bool enabled)
    {
        var index = FindLayerIndex(layerId);
        if (index < 0) return;
        ApplyJsonEdit(LayerPointer(index) + "/enabled", enabled);
    }

    private void AddLayer()
    {
        var layers = document?.Layers;
        if (document == null || layers == null) return;
        var copy = (JArray)layers.DeepClone();
        var id = UniqueLayerId(copy, "new-layer");
        copy.Add(new JObject
        {
            ["id"] = id,
            ["name"] = "New layer",
            ["enabled"] = true,
            ["source"] = new JObject
            {
                ["kind"] = "fbm",
                ["frequency"] = 4,
                ["octaves"] = 3,
                ["lacunarity"] = 2.0,
                ["gain"] = 0.5,
                ["offset"] = new JArray(0.0, 0.0),
                ["seed_domain"] = 1,
                ["domain_warp"] = null,
                ["cellular_jitter"] = 0.25,
            },
            ["remap"] = new JObject { ["input_min"] = -1.0, ["input_max"] = 1.0, ["invert"] = false, ["contrast"] = 1.0, ["bias"] = 0.0, ["clamp"] = true },
            ["mask"] = null,
            ["outputs"] = new JObject
            {
                ["height"] = new JObject { ["enabled"] = true, ["blend"] = new JObject { ["kind"] = "add" }, ["strength_m"] = 0.01 },
                ["albedo"] = new JObject
                {
                    ["enabled"] = false,
                    ["blend"] = "mix",
                    ["strength"] = 0.3,
                    ["colour_map"] = new JObject
                    {
                        ["kind"] = "ramp",
                        ["first"] = new JArray(0.25, 0.27, 0.24),
                        ["second"] = new JArray(0.42, 0.36, 0.28),
                    },
                    ["hue_influence"] = 0.0,
                    ["saturation_influence"] = 0.0,
                    ["value_influence"] = 0.0,
                }
            }
        });
        ReplaceLayers(copy, id);
    }

    private void DuplicateLayer()
    {
        var layers = document?.Layers;
        var selectedIndex = FindLayerIndex(document?.SelectedLayer);
        if (layers == null || selectedIndex < 0) return;
        var copy = (JArray)layers.DeepClone();
        var duplicate = (JObject)copy[selectedIndex].DeepClone();
        var id = UniqueLayerId(copy, document.GetLayerId(duplicate, selectedIndex) + "-copy");
        duplicate["id"] = id;
        duplicate["name"] = document.GetLayerName(duplicate, selectedIndex) + " Copy";
        copy.Insert(selectedIndex + 1, duplicate);
        ReplaceLayers(copy, id);
    }

    private void DeleteLayer()
    {
        var layers = document?.Layers;
        var selectedIndex = FindLayerIndex(document?.SelectedLayer);
        if (layers == null || selectedIndex < 0) return;
        var id = document.GetLayerId(layers[selectedIndex] as JObject, selectedIndex);
        var references = layers
            .OfType<JObject>()
            .Where(layer => string.Equals(layer["mask"]?["layer_id"]?.Value<string>(), id, StringComparison.Ordinal))
            .Select(layer => layer["name"]?.Value<string>() ?? "Unnamed layer")
            .ToArray();
        var message = references.Length == 0
            ? "Delete '" + document.GetLayerName(layers[selectedIndex] as JObject, selectedIndex) + "'?"
            : "The following masks reference this layer and will be broken:\n\n" + string.Join("\n", references) + "\n\nDelete anyway?";
        if (!EditorUtility.DisplayDialog("Delete procedural layer", message, "Delete", "Cancel")) return;
        var copy = (JArray)layers.DeepClone();
        copy.RemoveAt(selectedIndex);
        ReplaceLayers(copy, copy.Count == 0 ? string.Empty : document.GetLayerId(copy[Mathf.Clamp(selectedIndex, 0, copy.Count - 1)] as JObject, Mathf.Clamp(selectedIndex, 0, copy.Count - 1)));
    }

    private void ReorderLayer(int from, int to)
    {
        var layers = document?.Layers;
        if (layers == null || from < 0 || from >= layers.Count || to < 0 || to >= layers.Count) return;
        var copy = (JArray)layers.DeepClone();
        var moved = copy[from];
        copy.RemoveAt(from);
        copy.Insert(to, moved);
        if (!MasksAreEarlierOnly(copy))
        {
            EditorUtility.DisplayDialog("Cannot reorder layer", "A layer mask may only reference an earlier layer. Move its dependency first or clear the mask.", "OK");
            layerList?.Rebuild();
            return;
        }
        var selectedId = document.SelectedLayerId;
        ReplaceLayers(copy, selectedId);
    }

    private void ReplaceLayers(JArray layers, string selectedId)
    {
        var root = (JObject)document.Root.DeepClone();
        root["layers"] = layers;
        Undo.RecordObject(document, "Edit procedural material layers");
        document.SetJson(root.ToString(Formatting.None));
        if (!string.IsNullOrWhiteSpace(selectedId)) document.SelectLayer(selectedId);
    }

    private bool MasksAreEarlierOnly(JArray layers)
    {
        var known = new HashSet<string>(StringComparer.Ordinal);
        for (var index = 0; index < layers.Count; index++)
        {
            var layer = layers[index] as JObject;
            var mask = layer?["mask"] as JObject;
            if (string.Equals(mask?["kind"]?.Value<string>(), "layer", StringComparison.Ordinal)
                && !known.Contains(mask["layer_id"]?.Value<string>() ?? string.Empty)) return false;
            known.Add(document.GetLayerId(layer, index));
        }
        return true;
    }

    private void NewDocument()
    {
        SetDocument(ProceduralMaterialDocument.CreateNew(), replaced =>
        {
            if (replaced) SetStatus("New unsaved recipe.");
        });
    }

    private void OpenDocument()
    {
        var selected = EditorUtility.OpenFilePanel("Open procedural material recipe", RecipeDirectoryAbsolutePath(), "json");
        if (string.IsNullOrWhiteSpace(selected)) return;
        try
        {
            var loaded = ProceduralMaterialDocument.Load(selected);
            SetDocument(loaded, replaced =>
            {
                if (!replaced) return;
                EditorPrefs.SetString(LastRecipePreference, selected);
                SetStatus("Opened " + Path.GetFileName(selected) + ".");
            });
        }
        catch (Exception exception)
        {
            Debug.LogException(exception);
            EditorUtility.DisplayDialog("Open failed", exception.Message, "OK");
        }
    }

    private void SaveDocument(Action<bool> completed = null)
    {
        if (document == null)
        {
            completed?.Invoke(false);
            return;
        }
        if (string.IsNullOrWhiteSpace(document.SourceFilePath))
        {
            SaveAsDocument(completed);
            return;
        }
        if (document.HasExternalChanges())
        {
            var choice = EditorUtility.DisplayDialogComplex(
                "Recipe changed outside Unity",
                "Reload the external file, save as a new file, or explicitly overwrite the external changes.",
                "Reload",
                "Save As",
                "Overwrite");
            if (choice == 0)
            {
                document.ReplaceFromDisk();
                SetStatus("Reloaded external recipe changes.");
                completed?.Invoke(true);
                return;
            }
            if (choice == 1)
            {
                SaveAsDocument(completed);
                return;
            }
            if (choice != 2)
            {
                completed?.Invoke(false);
                return;
            }
            BeginValidatedSave(document, document.SourceFilePath, false, true, completed);
            return;
        }
        BeginValidatedSave(document, document.SourceFilePath, false, false, completed);
    }

    private void SaveAsDocument(Action<bool> completed = null)
    {
        if (document == null)
        {
            completed?.Invoke(false);
            return;
        }
        var directory = string.IsNullOrWhiteSpace(document.SourceFilePath)
            ? RecipeDirectoryAbsolutePath()
            : Path.GetDirectoryName(document.SourceFilePath);
        var path = EditorUtility.SaveFilePanel("Save procedural material recipe", directory, RecipeName() + ".json", "json");
        if (string.IsNullOrWhiteSpace(path))
        {
            completed?.Invoke(false);
            return;
        }
        BeginValidatedSave(document, path, true, true, completed);
    }

    private void BeginValidatedSave(
        ProceduralMaterialDocument target,
        string path,
        bool saveAs,
        bool overwriteExternalChanges,
        Action<bool> completed)
    {
        if (target == null || bakerClient == null)
        {
            completed?.Invoke(false);
            return;
        }

        var editGeneration = target.EditGeneration;
        SetStatus("Validating recipe with Rust before save…");
        try
        {
            bakerClient.RequestValidation(target, result =>
            {
                if (document != target)
                {
                    completed?.Invoke(false);
                    return;
                }
                DisplayDiagnostics(result);
                if (result == null || !result.Succeeded)
                {
                    SetStatus("Save blocked by Rust validation: " + (result?.Message ?? "No validation response."));
                    completed?.Invoke(false);
                    return;
                }
                if (target.EditGeneration != editGeneration)
                {
                    SetStatus("The recipe changed while it was being validated; save again to validate the latest edits.");
                    completed?.Invoke(false);
                    return;
                }

                var saveResult = saveAs
                    ? target.SaveAs(path)
                    : target.Save(overwriteExternalChanges);
                SaveResult(saveResult, completed);
            });
        }
        catch (Exception exception)
        {
            Debug.LogException(exception);
            SetStatus("Save validation could not start: " + exception.Message);
            completed?.Invoke(false);
        }
    }

    private void DuplicateDocument()
    {
        if (document == null) return;
        var directory = string.IsNullOrWhiteSpace(document.SourceFilePath) ? RecipeDirectoryAbsolutePath() : Path.GetDirectoryName(document.SourceFilePath);
        var path = EditorUtility.SaveFilePanel("Duplicate procedural material recipe", directory, RecipeName() + " Copy.json", "json");
        if (string.IsNullOrWhiteSpace(path)) return;
        var result = document.SaveCopy(path);
        if (result == ProceduralMaterialDocument.SaveResult.Succeeded)
        {
            SetStatus("Saved duplicate " + Path.GetFileName(path) + ".");
            try
            {
                SetDocument(ProceduralMaterialDocument.Load(path), replaced =>
                {
                    if (replaced) EditorPrefs.SetString(LastRecipePreference, path);
                });
            }
            catch (Exception exception)
            {
                Debug.LogException(exception);
                SetStatus("Saved duplicate, but it could not be opened: " + exception.Message);
            }
        }
        else SetStatus("Could not save duplicate.");
    }

    private void RevertDocument()
    {
        if (document == null || string.IsNullOrWhiteSpace(document.SourceFilePath)) return;
        if (document.IsDirty && !EditorUtility.DisplayDialog("Revert recipe", "Discard unsaved procedural material edits?", "Revert", "Cancel")) return;
        document.ReplaceFromDisk();
        SetStatus("Reverted to the saved recipe.");
    }

    private void ValidateDocument()
    {
        if (document == null) return;
        SetStatus("Validating recipe with Rust…");
        bakerClient.RequestValidation(document, result =>
        {
            DisplayDiagnostics(result);
            SetStatus(result.Succeeded ? "Rust validation passed." : "Rust validation failed: " + result.Message);
        });
    }

    private void PreviewDocument()
    {
        if (document == null || previewController == null) return;
        SetStatus("Validating before preview…");
        bakerClient.RequestValidation(document, result =>
        {
            DisplayDiagnostics(result);
            if (!result.Succeeded)
            {
                SetStatus("Preview blocked by Rust validation.");
                return;
            }
            SetStatus("Rendering Rust preview…");
            previewController.PreviewNow();
        });
    }

    private void BakeDocument()
    {
        if (document == null) return;
        var bakeDocument = document;
        var bakeGeneration = bakeDocument.EditGeneration;
        var outputAssetPath = outputField?.value;
        if (!TryGetSafeGeneratedOutputPath(outputAssetPath, out var outputAbsolutePath))
        {
            EditorUtility.DisplayDialog(
                "Invalid bake output",
                "Output must be a canonical directory below " + OutputRootAssetPath + ", without '..' segments or symlink escapes.",
                "OK");
            return;
        }
        var profile = profileField?.value ?? "motu_unity_terrain";
        if (assignToggle?.value == true && !IsMaterialAssignmentProfile(profile))
        {
            EditorUtility.DisplayDialog(
                "Material assignment unavailable",
                "Material assignment requires the motu_unity_terrain profile because that profile generates the packed mask map. Choose that profile or disable assignment.",
                "OK");
            return;
        }
        try
        {
            Directory.CreateDirectory(outputAbsolutePath);
            if (ContainsReparsePointBelow(outputAbsolutePath))
            {
                SetStatus("Bake output contains a symlink or reparse point; choose a clean generated-texture directory.");
                return;
            }
        }
        catch (Exception exception)
        {
            SetStatus("Bake output could not be created: " + exception.Message);
            return;
        }
        var replaceExisting = replaceToggle?.value ?? false;
        bool hasExistingMaps;
        try
        {
            hasExistingMaps = Directory.EnumerateFiles(outputAbsolutePath, "*.png", SearchOption.AllDirectories).Any();
        }
        catch (Exception exception)
        {
            SetStatus("Bake output could not be inspected: " + exception.Message);
            return;
        }
        if (hasExistingMaps && !replaceExisting)
        {
            EditorUtility.DisplayDialog("Replace Existing Set required", "The output folder already contains maps. Enable Replace Existing Set before baking.", "OK");
            return;
        }
        SetStatus("Validating before bake…");
        bakerClient.RequestValidation(bakeDocument, validation =>
        {
            DisplayDiagnostics(validation);
            if (document != bakeDocument)
            {
                SetStatus("Bake cancelled because the active document changed.");
                return;
            }
            if (validation == null || !validation.Succeeded)
            {
                SetStatus("Bake blocked by Rust validation.");
                return;
            }
            if (bakeDocument.EditGeneration != bakeGeneration)
            {
                SetStatus("Bake cancelled because the recipe changed while it was being validated.");
                return;
            }
            SetStatus("Baking procedural material asynchronously…");
            bakerClient.RequestBake(bakeDocument, outputAbsolutePath, profile, replaceExisting, result =>
            {
                DisplayDiagnostics(result);
                if (document != bakeDocument)
                {
                    SetStatus("Bake completed for a document that is no longer active; no assignment was made.");
                    return;
                }
                if (result == null || !result.Succeeded)
                {
                    SetStatus("Bake failed: " + (result?.Message ?? "No bake response."));
                    return;
                }
                var manifestPath = FindManifest(outputAbsolutePath, result.ManifestPath);
                if (string.IsNullOrWhiteSpace(manifestPath))
                {
                    SetStatus("Bake failed: completion manifest was not found; no assets were imported.");
                    return;
                }
                try
                {
                    var imported = ImportGeneratedTextures(outputAbsolutePath, manifestPath);
                    if (imported.Count == 0) throw new InvalidOperationException("The completion manifest contained no importable PNG maps.");
                    if (assignToggle?.value == true) AssignToMaterial(imported, profile);
                    bakeDocument.SetLastBakeHash(string.IsNullOrWhiteSpace(result.RecipeHash) ? bakeDocument.CurrentHash : result.RecipeHash);
                    SetStatus("Bake completed and " + imported.Count + " maps imported.");
                }
                catch (Exception exception)
                {
                    Debug.LogException(exception, this);
                    SetStatus("Bake output was not imported: " + exception.Message);
                }
            });
        });
    }

    private void DisplayDiagnostics(RustTextureBakerClient.BakerResult result)
    {
        if (diagnosticsList == null) return;
        diagnosticsList.Clear();
        if (result == null)
        {
            diagnosticsList.Add(new Label("The baker returned no response."));
            return;
        }
        foreach (var diagnostic in result.Diagnostics)
        {
            var label = new Label(string.IsNullOrWhiteSpace(diagnostic.Pointer)
                ? diagnostic.Code + ": " + diagnostic.Message
                : diagnostic.Pointer + " • " + diagnostic.Code + ": " + diagnostic.Message);
            label.AddToClassList(diagnostic.IsError ? "diagnostic-error" : "diagnostic-warning");
            if (!string.IsNullOrWhiteSpace(diagnostic.Pointer))
            {
                label.tooltip = "Click to focus the affected control.";
                var pointer = diagnostic.Pointer;
                label.RegisterCallback<ClickEvent>(_ => FocusDiagnostic(pointer));
            }
            diagnosticsList.Add(label);
        }
        if (result.Diagnostics.Count == 0 && !result.Succeeded && !string.IsNullOrWhiteSpace(result.Message)) diagnosticsList.Add(new Label(result.Message));
        var firstError = result.Diagnostics.FirstOrDefault(diagnostic => diagnostic.IsError && !string.IsNullOrWhiteSpace(diagnostic.Pointer));
        if (firstError != null) EditorApplication.delayCall += () => FocusDiagnostic(firstError.Pointer);
    }

    private void FocusDiagnostic(string pointer)
    {
        if (document == null || string.IsNullOrWhiteSpace(pointer)) return;
        var segments = pointer.Trim('/').Split('/');
        var layerIndex = -1;
        var layerPointer = segments.Length > 1
            && string.Equals(segments[0], "layers", StringComparison.Ordinal)
            && int.TryParse(segments[1], out layerIndex);
        if (layerPointer
            && document.Layers is JArray layers
            && layerIndex >= 0
            && layerIndex < layers.Count
            && layers[layerIndex] is JObject layer)
        {
            var layerId = document.GetLayerId(layer, layerIndex);
            if (!string.Equals(document.SelectedLayerId, layerId, StringComparison.Ordinal)) document.SelectLayer(layerId);
        }
        SetInspectorMode(layerPointer);
        var control = rootVisualElement.Query<VisualElement>()
            .ToList()
            .FirstOrDefault(element => string.Equals(element.userData as string, pointer, StringComparison.Ordinal));
        if (control == null)
        {
            SetStatus("Validation issue at " + pointer + "; no visible control is available for the current variant.");
            return;
        }
        inspectorPanel?.ScrollTo(control);
        control.Focus();
    }

    private void SaveResult(ProceduralMaterialDocument.SaveResult result, Action<bool> completed = null)
    {
        switch (result)
        {
            case ProceduralMaterialDocument.SaveResult.Succeeded:
                EditorPrefs.SetString(LastRecipePreference, document.SourceFilePath);
                SetStatus("Saved " + Path.GetFileName(document.SourceFilePath) + ".");
                completed?.Invoke(true);
                break;
            case ProceduralMaterialDocument.SaveResult.ExternalChanges:
                SetStatus("Save stopped because the recipe changed externally.");
                completed?.Invoke(false);
                break;
            case ProceduralMaterialDocument.SaveResult.SaveAsRequired:
                SaveAsDocument(completed);
                break;
            default:
                SetStatus("Save failed.");
                completed?.Invoke(false);
                break;
        }
    }

    private void ConfirmDiscardIfNeeded(Action<bool> completed)
    {
        if (document == null || !document.IsDirty)
        {
            completed?.Invoke(true);
            return;
        }
        var choice = EditorUtility.DisplayDialogComplex("Unsaved procedural material edits", "Save your current recipe before continuing?", "Save", "Discard", "Cancel");
        if (choice == 0)
        {
            SaveDocument(saved => completed?.Invoke(saved && document != null && !document.IsDirty));
            return;
        }
        completed?.Invoke(choice == 1);
    }

    private void SetStatus(string message)
    {
        if (statusLabel != null) statusLabel.text = message ?? string.Empty;
    }

    private int FindLayerIndex(JObject layer)
    {
        if (layer == null || document?.Layers == null) return -1;
        for (var index = 0; index < document.Layers.Count; index++) if (ReferenceEquals(document.Layers[index], layer)) return index;
        return FindLayerIndex(document.GetLayerId(layer, 0));
    }

    private int FindLayerIndex(string layerId)
    {
        var layers = document?.Layers;
        if (layers == null) return -1;
        for (var index = 0; index < layers.Count; index++) if (string.Equals(document.GetLayerId(layers[index] as JObject, index), layerId, StringComparison.Ordinal)) return index;
        return -1;
    }

    private string LayerPointer(int index)
    {
        return "/layers/" + index;
    }

    private static string UniqueLayerId(JArray layers, string proposed)
    {
        var id = proposed;
        var suffix = 2;
        while (layers.OfType<JObject>().Any(layer => string.Equals(layer["id"]?.Value<string>(), id, StringComparison.Ordinal))) id = proposed + "-" + suffix++;
        return id;
    }

    private string RecipeName()
    {
        var name = document?.Root["name"]?.Value<string>();
        return string.IsNullOrWhiteSpace(name) ? "ProceduralMaterial" : name;
    }

    private static string FindDefaultRecipe()
    {
        var path = Path.Combine(RecipeDirectoryAbsolutePath(), "cracked-stone.json");
        return File.Exists(path) ? path : string.Empty;
    }

    private static string RecipeDirectoryAbsolutePath()
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        return Path.GetFullPath(Path.Combine(projectRoot, "..", RecipeDirectoryName));
    }

    private static bool IsOutputPathUnderGeneratedRoot(string assetPath)
    {
        if (string.IsNullOrWhiteSpace(assetPath)) return false;
        var normalized = assetPath.Replace('\\', '/').TrimEnd('/');
        if (Path.IsPathRooted(normalized)) return false;
        var segments = normalized.Split('/');
        if (segments.Any(segment => string.Equals(segment, ".", StringComparison.Ordinal) || string.Equals(segment, "..", StringComparison.Ordinal))) return false;
        return normalized.Equals(OutputRootAssetPath, StringComparison.OrdinalIgnoreCase)
            || normalized.StartsWith(OutputRootAssetPath + "/", StringComparison.OrdinalIgnoreCase);
    }

    private static bool TryGetSafeGeneratedOutputPath(string assetPath, out string absolutePath)
    {
        absolutePath = string.Empty;
        if (!IsOutputPathUnderGeneratedRoot(assetPath)) return false;
        var normalized = assetPath.Replace('\\', '/').TrimEnd('/');
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        var generatedRoot = Path.GetFullPath(Path.Combine(projectRoot, OutputRootAssetPath));
        var candidate = Path.GetFullPath(Path.Combine(projectRoot, normalized));
        if (!IsSameOrUnderDirectory(candidate, generatedRoot)) return false;
        if (File.Exists(candidate)) return false;
        if (ContainsReparsePoint(candidate, projectRoot)) return false;
        absolutePath = candidate;
        return true;
    }

    private static string FindManifest(string outputDirectory, string reportedPath)
    {
        if (!string.IsNullOrWhiteSpace(reportedPath))
        {
            var candidate = Path.IsPathRooted(reportedPath) ? reportedPath : Path.Combine(outputDirectory, reportedPath);
            if (File.Exists(candidate)
                && IsPathUnderDirectory(candidate, outputDirectory)
                && !ContainsReparsePoint(candidate, outputDirectory)) return candidate;
        }
        foreach (var path in Directory.EnumerateFiles(outputDirectory, "*.json", SearchOption.AllDirectories))
        {
            if (ContainsReparsePoint(path, outputDirectory)) continue;
            if (Path.GetFileName(path).IndexOf("manifest", StringComparison.OrdinalIgnoreCase) >= 0) return path;
            try
            {
                var token = JToken.Parse(File.ReadAllText(path));
                if (token is JObject objectToken && objectToken["maps"] != null && objectToken["dimensions"] != null) return path;
            }
            catch (JsonException) { }
        }
        return string.Empty;
    }

    private static List<string> ImportGeneratedTextures(string outputDirectory, string manifestPath)
    {
        if (string.IsNullOrWhiteSpace(manifestPath) || !File.Exists(manifestPath)) throw new InvalidOperationException("A successful completion manifest is required before importing maps.");
        var manifest = JObject.Parse(File.ReadAllText(manifestPath));
        if (!(manifest["maps"] is JArray mapEntries) || mapEntries.Count == 0)
        {
            throw new InvalidOperationException("The completion manifest did not list any generated maps.");
        }
        var generatedFiles = new List<string>();
        foreach (var mapEntry in mapEntries.OfType<JObject>())
        {
            var relativeFile = mapEntry["file"]?.Value<string>();
            if (string.IsNullOrWhiteSpace(relativeFile)) throw new InvalidOperationException("The completion manifest contains a map without a file.");
            var absolutePath = Path.GetFullPath(Path.Combine(outputDirectory, relativeFile));
            if (!IsPathUnderDirectory(absolutePath, outputDirectory)
                || ContainsReparsePoint(absolutePath, outputDirectory)
                || !File.Exists(absolutePath))
            {
                throw new InvalidOperationException("The completion manifest references a missing or unsafe map: " + relativeFile);
            }
            generatedFiles.Add(absolutePath);
        }
        var imported = new List<string>();
        AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
        foreach (var absolutePath in generatedFiles.Distinct(StringComparer.OrdinalIgnoreCase))
        {
            var assetPath = FileUtil.GetProjectRelativePath(absolutePath).Replace('\\', '/');
            if (!IsOutputPathUnderGeneratedRoot(assetPath)) throw new InvalidOperationException("Generated map escaped the Assets/Generated/Textures root: " + assetPath);
            AssetDatabase.ImportAsset(assetPath, ImportAssetOptions.ForceSynchronousImport);
            var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
            if (importer == null) throw new InvalidOperationException("No texture importer was created for " + assetPath + ".");
            ConfigureImporter(importer, assetPath);
            imported.Add(assetPath);
        }
        AssetDatabase.SaveAssets();
        return imported;
    }

    private static bool IsPathUnderDirectory(string candidate, string directory)
    {
        var root = Path.GetFullPath(directory).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar) + Path.DirectorySeparatorChar;
        var normalizedCandidate = Path.GetFullPath(candidate);
        return normalizedCandidate.StartsWith(root, StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsSameOrUnderDirectory(string candidate, string directory)
    {
        var normalizedCandidate = Path.GetFullPath(candidate).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        var normalizedDirectory = Path.GetFullPath(directory).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        return string.Equals(normalizedCandidate, normalizedDirectory, StringComparison.OrdinalIgnoreCase)
            || normalizedCandidate.StartsWith(normalizedDirectory + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase);
    }

    private static bool ContainsReparsePoint(string candidate, string stopDirectory)
    {
        var current = Path.GetFullPath(candidate);
        var stop = Path.GetFullPath(stopDirectory).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        while (true)
        {
            if (File.Exists(current) || Directory.Exists(current))
            {
                try
                {
                    if ((File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0) return true;
                }
                catch (IOException) { return true; }
                catch (UnauthorizedAccessException) { return true; }
            }
            if (string.Equals(current.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar), stop, StringComparison.OrdinalIgnoreCase)) return false;
            var parent = Path.GetDirectoryName(current);
            if (string.IsNullOrWhiteSpace(parent) || string.Equals(parent, current, StringComparison.OrdinalIgnoreCase)) return true;
            current = parent;
        }
    }

    private static bool ContainsReparsePointBelow(string directory)
    {
        if (!Directory.Exists(directory)) return false;
        var pending = new Stack<string>();
        pending.Push(directory);
        while (pending.Count > 0)
        {
            var current = pending.Pop();
            try
            {
                foreach (var entry in Directory.EnumerateFileSystemEntries(current))
                {
                    if ((File.GetAttributes(entry) & FileAttributes.ReparsePoint) != 0) return true;
                    if (Directory.Exists(entry)) pending.Push(entry);
                }
            }
            catch (IOException) { return true; }
            catch (UnauthorizedAccessException) { return true; }
        }
        return false;
    }

    public static void ConfigureCommittedGeneratedTextures()
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        var generatedRoot = Path.Combine(projectRoot, OutputRootAssetPath);
        if (!Directory.Exists(generatedRoot)) throw new DirectoryNotFoundException(generatedRoot);
        AssetDatabase.Refresh(ImportAssetOptions.ForceSynchronousImport);
        foreach (var absolutePath in Directory.EnumerateFiles(generatedRoot, "*.png", SearchOption.AllDirectories))
        {
            var assetPath = FileUtil.GetProjectRelativePath(absolutePath).Replace('\\', '/');
            var importer = AssetImporter.GetAtPath(assetPath) as TextureImporter;
            if (importer == null) throw new InvalidOperationException("No texture importer was created for " + assetPath + ".");
            ConfigureImporter(importer, assetPath);
        }
        AssetDatabase.SaveAssets();
    }

    private static void ConfigureImporter(TextureImporter importer, string assetPath)
    {
        var filename = Path.GetFileName(assetPath);
        var isNormal = filename.EndsWith("_normal.png", StringComparison.OrdinalIgnoreCase);
        var isAlbedo = filename.EndsWith("_albedo.png", StringComparison.OrdinalIgnoreCase);
        importer.textureType = isNormal ? TextureImporterType.NormalMap : TextureImporterType.Default;
        importer.sRGBTexture = isAlbedo;
        importer.mipmapEnabled = true;
        importer.wrapMode = TextureWrapMode.Repeat;
        importer.filterMode = FilterMode.Bilinear;
        importer.textureCompression = TextureImporterCompression.Uncompressed;
        importer.alphaSource = TextureImporterAlphaSource.None;
        importer.SaveAndReimport();
    }

    private void AssignToMaterial(IReadOnlyList<string> imported, string profile)
    {
        if (!IsMaterialAssignmentProfile(profile))
        {
            throw new InvalidOperationException("Material assignment requires the motu_unity_terrain profile and its packed mask map.");
        }
        var material = materialField?.value as Material;
        if (material == null) throw new InvalidOperationException("Select a material before enabling assignment.");
        var albedoPath = FindImportedMap(imported, "_albedo.png");
        var normalPath = FindImportedMap(imported, "_normal.png");
        var maskPath = FindImportedMap(imported, "_mask.png");
        if (albedoPath == null || normalPath == null || maskPath == null)
        {
            throw new InvalidOperationException("Material assignment requires albedo, normal, and packed mask maps; the selected bake profile did not produce a complete terrain set.");
        }
        var albedo = AssetDatabase.LoadAssetAtPath<Texture2D>(albedoPath);
        var normal = AssetDatabase.LoadAssetAtPath<Texture2D>(normalPath);
        var mask = AssetDatabase.LoadAssetAtPath<Texture2D>(maskPath);
        if (albedo == null || normal == null || mask == null)
        {
            throw new InvalidOperationException("Material assignment could not load the complete albedo, normal, and packed mask map set after import.");
        }
        var rock = string.Equals(assignmentTarget?.value, "Rock", StringComparison.OrdinalIgnoreCase);
        var albedoProperty = rock ? "_RockAlbedoMap" : "_RiverBedAlbedoMap";
        var normalProperty = rock ? "_RockNormalMap" : "_RiverBedNormalMap";
        var maskProperty = rock ? "_RockMaskMap" : "_RiverBedMaskMap";
        if (!material.HasProperty(albedoProperty) || !material.HasProperty(normalProperty) || !material.HasProperty(maskProperty)) throw new InvalidOperationException("Material does not expose the selected terrain texture properties.");
        Undo.RecordObject(material, "Assign procedural terrain textures");
        material.SetTexture(albedoProperty, albedo);
        material.SetTexture(normalProperty, normal);
        material.SetTexture(maskProperty, mask);
        EditorUtility.SetDirty(material);
        AssetDatabase.SaveAssets();
    }

    private static bool IsMaterialAssignmentProfile(string profile)
    {
        return string.Equals(profile, "motu_unity_terrain", StringComparison.OrdinalIgnoreCase);
    }

    private static string FindImportedMap(IEnumerable<string> imported, string suffix)
    {
        return imported.FirstOrDefault(path => Path.GetFileName(path).EndsWith(suffix, StringComparison.OrdinalIgnoreCase));
    }

    private static void BuildFallbackUi(VisualElement root)
    {
        var label = new Label("Procedural Material Studio UI asset could not be loaded. Reimport the editor folder.");
        label.AddToClassList("diagnostic-error");
        root.Add(label);
    }
}
