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
/// 2D map tabs, seam tiling and a lightweight PreviewRenderUtility lit view.
/// Every image is loaded from Library/ProceduralMaterialPreview and remains
/// outside Unity's AssetDatabase until an explicit bake is requested.
/// </summary>
public sealed class ProceduralMaterialPreviewController : IDisposable
{
    private readonly RustTextureBakerClient bakerClient;
    private readonly VisualElement root = new VisualElement();
    private readonly VisualElement imageHost = new VisualElement();
    private readonly Label details = new Label("No preview generated.");
    private readonly Dictionary<string, PreviewCacheEntry> cache = new Dictionary<string, PreviewCacheEntry>(StringComparer.Ordinal);
    private readonly LinkedList<string> cacheOrder = new LinkedList<string>();
    private readonly List<ToolbarButton> tabButtons = new List<ToolbarButton>();
    private readonly LinkedList<string> timingHistory = new LinkedList<string>();
    private readonly string[] tabs = { "Albedo", "Height", "Normal", "Occlusion", "Mask", "Layer", "Lit" };
    private ProceduralMaterialDocument document;
    private ToolbarToggle autoPreviewToggle;
    private Toggle tiledToggle;
    private Slider zoomSlider;
    private PopupField<string> objectField;
    private PopupField<string> maskChannelField;
    private VisualElement mapSettings;
    private VisualElement litSettings;
    private IMGUIContainer litPreview;
    private Label telemetry;
    private PreviewRenderUtility renderUtility;
    private Mesh planeMesh;
    private Mesh sphereMesh;
    private Material previewMaterial;
    private Texture2D activeTexture;
    private Texture2D maskChannelTexture;
    private Texture2D maskChannelSource;
    private string activeMaskChannel = "RGBA";
    private string activeTab = "Albedo";
    private string lastRecipeHash = string.Empty;
    private int previewResolution = 256;
    private bool autoPreview = true;
    private bool disposed;
    private bool previewScheduled;
    private double previewDue;
    private int previewRequestId;
    private bool soloLayer;
    private bool beforeAfter;
    private ProceduralMaterialDocument soloPreviewDocument;
    private PreviewCacheEntry previousPreviewEntry;
    private PreviewCacheEntry lastPreviewEntry;
    private string lastPreviewKey = string.Empty;
    private Vector2 litOrbit = new Vector2(25f, -30f);
    private float litDistance = 2.8f;
    private float lightDirection = -35f;
    private float lightStrength = 1.2f;
    private bool useLitNormal = true;
    private bool useLitOcclusion = true;
    private bool useLitHeight = true;
    private Vector2 mapPan;
    private Vector2 panStart;
    private Vector2 pointerStart;
    private int panPointerId = -1;

    public ProceduralMaterialPreviewController(RustTextureBakerClient bakerClient)
    {
        this.bakerClient = bakerClient ?? throw new ArgumentNullException(nameof(bakerClient));
        BuildUi();
        EditorApplication.update += Tick;
    }

    public VisualElement Root => root;
    public int PreviewResolution => previewResolution;
    public bool AutoPreview => autoPreview;

    public void ToggleSoloLayer()
    {
        soloLayer = !soloLayer;
        details.text = soloLayer ? "Solo layer preview enabled." : "Solo layer preview disabled.";
        if (document != null) SchedulePreview();
    }

    public void ToggleBeforeAfter()
    {
        beforeAfter = !beforeAfter;
        details.text = beforeAfter ? "Before / after comparison enabled." : "Before / after comparison disabled.";
        RefreshDisplay();
    }

    public void SetDocument(ProceduralMaterialDocument value)
    {
        if (!ReferenceEquals(document, value))
        {
            DestroyMaskChannelTexture();
            ++previewRequestId;
            bakerClient.CancelPreview();
            DestroySoloPreviewDocument();
            previewScheduled = false;
            lastRecipeHash = string.Empty;
            previousPreviewEntry = null;
            lastPreviewEntry = null;
            lastPreviewKey = string.Empty;
            foreach (var entry in cache.Values.ToArray()) DestroyTextureSet(entry);
            cache.Clear();
            cacheOrder.Clear();
            RefreshTelemetry();
        }
        document = value;
        activeTexture = null;
        RefreshDisplay();
    }

    public void SetAutoPreview(bool value)
    {
        autoPreview = value;
        if (autoPreview && document != null) SchedulePreview();
    }

    public void SetResolution(int value)
    {
        previewResolution = Mathf.Clamp(value, 128, 512);
        if (document != null) SchedulePreview();
    }

    public void NotifyDocumentChanged()
    {
        if (document == null) return;
        if (bakerClient.IsPreviewRunning)
        {
            ++previewRequestId;
            bakerClient.CancelPreview();
            DestroySoloPreviewDocument();
        }
        activeTexture = null;
        RefreshDisplay();
        if (autoPreview) SchedulePreview();
    }

    public void PreviewNow()
    {
        if (document == null) return;
        previewScheduled = false;
        bakerClient.CancelPreview();
        ++previewRequestId;
        if (TryUseCachedPreview()) return;
        var generation = document.EditGeneration;
        var hash = document.CurrentHash;
        var requestId = previewRequestId;
        DestroySoloPreviewDocument();
        var requestDocument = soloLayer ? CreateSoloPreviewDocument(document) : document;
        details.text = "Rendering " + previewResolution + "×" + previewResolution + " preview…";
        bakerClient.RequestPreview(requestDocument, previewResolution, result =>
        {
            DestroySoloPreviewDocument();
            if (disposed || requestId != previewRequestId || document == null || document.EditGeneration != generation || !string.Equals(document.CurrentHash, hash, StringComparison.Ordinal))
            {
                bakerClient.ReleasePreviewDirectory(result?.OutputDirectory);
                return;
            }
            if (!result.Succeeded)
            {
                bakerClient.ReleasePreviewDirectory(result.OutputDirectory);
                details.text = "Preview failed: " + result.Message;
                return;
            }
            lastRecipeHash = string.IsNullOrWhiteSpace(result.RecipeHash) ? hash : result.RecipeHash;
            var entry = LoadPreview(result, hash);
            if (entry == null)
            {
                bakerClient.ReleasePreviewDirectory(result.OutputDirectory);
                details.text = "Preview completed without any map files.";
                return;
            }
            var cacheKey = hash + "|" + previewResolution + "|" + document.SelectedLayerId + "|solo=" + soloLayer;
            previousPreviewEntry = string.Equals(lastPreviewKey, cacheKey, StringComparison.Ordinal) ? null : lastPreviewEntry;
            AddToCache(cacheKey, entry);
            lastPreviewEntry = entry;
            lastPreviewKey = cacheKey;
            RecordTiming(result);
            details.text = "Preview " + previewResolution + "×" + previewResolution + " • tile " + TileSummary() + " • " + lastRecipeHash.Substring(0, Math.Min(12, lastRecipeHash.Length));
            RefreshDisplay();
        });
    }

    public void SetActiveTab(string tab)
    {
        if (string.IsNullOrWhiteSpace(tab) || !tabs.Contains(tab)) return;
        activeTab = tab;
        RefreshDisplay();
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        EditorApplication.update -= Tick;
        bakerClient.CancelPreview();
        DestroySoloPreviewDocument();
        foreach (var entry in cache.Values.ToArray()) DestroyTextureSet(entry);
        cache.Clear();
        cacheOrder.Clear();
        if (previewMaterial != null) UnityEngine.Object.DestroyImmediate(previewMaterial);
        DestroyMaskChannelTexture();
        if (planeMesh != null) UnityEngine.Object.DestroyImmediate(planeMesh);
        if (sphereMesh != null) UnityEngine.Object.DestroyImmediate(sphereMesh);
        renderUtility?.Cleanup();
        renderUtility = null;
    }

    private void BuildUi()
    {
        root.name = "procedural-material-preview";
        root.AddToClassList("preview-panel");
        var heading = new Label("Preview");
        heading.AddToClassList("panel-heading");
        root.Add(heading);

        var toolbar = new Toolbar();
        foreach (var tab in tabs)
        {
            var tabName = tab;
            var button = new ToolbarButton(() => SetActiveTab(tabName)) { text = tabName };
            button.AddToClassList("preview-tab");
            tabButtons.Add(button);
            toolbar.Add(button);
        }
        root.Add(toolbar);

        var settings = new VisualElement();
        settings.AddToClassList("preview-settings");
        autoPreviewToggle = new ToolbarToggle { text = "Auto Preview", value = autoPreview };
        autoPreviewToggle.RegisterValueChangedCallback(change => SetAutoPreview(change.newValue));
        settings.Add(autoPreviewToggle);
        var resolution = new PopupField<int>("Resolution", new List<int> { 128, 256, 512 }, 1);
        resolution.AddToClassList("preview-popup");
        resolution.RegisterValueChangedCallback(change => SetResolution(change.newValue));
        settings.Add(resolution);
        root.Add(settings);

        litSettings = new VisualElement { name = "preview-lit-settings" };
        litSettings.AddToClassList("preview-settings");
        objectField = new PopupField<string>("Object", new List<string> { "Sphere", "Plane" }, 0);
        objectField.AddToClassList("preview-popup");
        objectField.RegisterValueChangedCallback(_ => litPreview?.MarkDirtyRepaint());
        litSettings.Add(objectField);
        var normalToggle = new Toggle("Normal") { value = useLitNormal };
        normalToggle.RegisterValueChangedCallback(change =>
        {
            useLitNormal = change.newValue;
            litPreview?.MarkDirtyRepaint();
        });
        litSettings.Add(normalToggle);
        var occlusionToggle = new Toggle("AO") { value = useLitOcclusion };
        occlusionToggle.RegisterValueChangedCallback(change =>
        {
            useLitOcclusion = change.newValue;
            litPreview?.MarkDirtyRepaint();
        });
        litSettings.Add(occlusionToggle);
        var heightToggle = new Toggle("Height") { value = useLitHeight };
        heightToggle.RegisterValueChangedCallback(change =>
        {
            useLitHeight = change.newValue;
            litPreview?.MarkDirtyRepaint();
        });
        litSettings.Add(heightToggle);
        var lightDirectionSlider = new Slider("Light", -180f, 180f) { value = lightDirection };
        lightDirectionSlider.AddToClassList("preview-slider");
        lightDirectionSlider.RegisterValueChangedCallback(change =>
        {
            lightDirection = change.newValue;
            litPreview?.MarkDirtyRepaint();
        });
        litSettings.Add(lightDirectionSlider);
        var lightStrengthSlider = new Slider("Strength", 0f, 3f) { value = lightStrength };
        lightStrengthSlider.AddToClassList("preview-slider");
        lightStrengthSlider.RegisterValueChangedCallback(change =>
        {
            lightStrength = change.newValue;
            litPreview?.MarkDirtyRepaint();
        });
        litSettings.Add(lightStrengthSlider);
        root.Add(litSettings);

        mapSettings = new VisualElement { name = "preview-map-settings" };
        mapSettings.AddToClassList("preview-settings");
        tiledToggle = new Toggle("2×2 seam view") { value = false };
        tiledToggle.RegisterValueChangedCallback(change => RefreshDisplay());
        mapSettings.Add(tiledToggle);
        zoomSlider = new Slider("Zoom", 0.25f, 4f) { value = 1f };
        zoomSlider.AddToClassList("preview-slider");
        zoomSlider.RegisterValueChangedCallback(change => RefreshDisplay());
        mapSettings.Add(zoomSlider);
        maskChannelField = new PopupField<string>("Mask channel", new List<string> { "RGBA", "R Height", "G Occlusion", "B Spare", "A Opacity" }, 0);
        maskChannelField.AddToClassList("preview-popup-wide");
        maskChannelField.RegisterValueChangedCallback(change =>
        {
            activeMaskChannel = change.newValue;
            DestroyMaskChannelTexture();
            RefreshDisplay();
        });
        mapSettings.Add(maskChannelField);
        root.Add(mapSettings);

        imageHost.name = "preview-image-host";
        imageHost.AddToClassList("preview-image-host");
        root.Add(imageHost);
        litPreview = new IMGUIContainer(DrawLitPreview);
        litPreview.name = "preview-lit-view";
        litPreview.style.flexGrow = 1;
        litPreview.style.minHeight = 260f;
        root.Add(litPreview);
        root.Add(details);
        var diagnostics = new Foldout { text = "Preview diagnostics", value = false };
        telemetry = new Label();
        diagnostics.Add(telemetry);
        root.Add(diagnostics);
        RefreshTelemetry();
        RefreshDisplay();
    }

    private void Tick()
    {
        if (disposed || !previewScheduled || !autoPreview || EditorApplication.timeSinceStartup < previewDue) return;
        previewScheduled = false;
        if (bakerClient.UseCargoFallback)
        {
            details.text = "Auto Preview is paused while Cargo fallback is selected; use Preview manually or configure a release baker.";
            return;
        }
        PreviewNow();
    }

    private void SchedulePreview()
    {
        if (TryUseCachedPreview())
        {
            previewScheduled = false;
            return;
        }
        previewDue = EditorApplication.timeSinceStartup + 0.3;
        previewScheduled = true;
        if (bakerClient.UseCargoFallback) details.text = "Auto Preview requires a release baker (Cargo remains available for manual Preview/Bake).";
    }

    private void RefreshDisplay()
    {
        if (disposed) return;
        foreach (var button in tabButtons) button.EnableInClassList("preview-tab-active", button.text == activeTab);
        var showingLit = string.Equals(activeTab, "Lit", StringComparison.Ordinal);
        if (litSettings != null) litSettings.style.display = showingLit ? DisplayStyle.Flex : DisplayStyle.None;
        if (mapSettings != null) mapSettings.style.display = showingLit ? DisplayStyle.None : DisplayStyle.Flex;
        if (maskChannelField != null) maskChannelField.style.display = string.Equals(activeTab, "Mask", StringComparison.Ordinal) ? DisplayStyle.Flex : DisplayStyle.None;
        imageHost.Clear();
        litPreview.style.display = showingLit ? DisplayStyle.Flex : DisplayStyle.None;
        imageHost.style.display = showingLit ? DisplayStyle.None : DisplayStyle.Flex;
        if (showingLit)
        {
            litPreview.MarkDirtyRepaint();
            return;
        }

        var entry = FindCurrentCacheEntry();
        if (entry == null)
        {
            imageHost.Add(new HelpBox("Run Preview to render Rust-owned maps.", HelpBoxMessageType.Info));
            return;
        }
        var path = ResolveMapPath(entry, activeTab);
        if (string.IsNullOrWhiteSpace(path) || !File.Exists(path))
        {
            imageHost.Add(new HelpBox("No " + activeTab + " map was returned for this recipe.", HelpBoxMessageType.Warning));
            return;
        }
        var linear = !string.Equals(activeTab, "Albedo", StringComparison.Ordinal);
        activeTexture = entry.GetTexture(path, linear);
        if (activeTab == "Mask") activeTexture = MaskDisplayTexture(activeTexture);
        if (activeTexture == null)
        {
            imageHost.Add(new HelpBox("Could not load " + Path.GetFileName(path) + ".", HelpBoxMessageType.Error));
            return;
        }
        if (beforeAfter && previousPreviewEntry != null)
        {
            var beforePath = ResolveMapPath(previousPreviewEntry, activeTab);
            if (!string.IsNullOrWhiteSpace(beforePath) && File.Exists(beforePath))
            {
                var before = previousPreviewEntry.GetTexture(beforePath, linear);
                if (before != null)
                {
                    BuildComparisonMap(before, activeTexture);
                    return;
                }
            }
        }
        if (tiledToggle != null && tiledToggle.value) BuildTiledMap(activeTexture);
        else BuildSingleMap(activeTexture);
    }

    private void BuildSingleMap(Texture2D texture)
    {
        var image = new Image { image = texture, scaleMode = ScaleMode.ScaleToFit };
        var dimension = Mathf.Max(128f, texture.width * (zoomSlider?.value ?? 1f));
        image.style.width = dimension;
        image.style.height = dimension;
        ConfigureMapInteraction(image, texture);
        imageHost.Add(image);
    }

    private void BuildTiledMap(Texture2D texture)
    {
        var grid = new VisualElement();
        grid.AddToClassList("preview-tile-grid");
        var dimension = Mathf.Max(128f, texture.width * (zoomSlider?.value ?? 1f) * 0.5f);
        for (var index = 0; index < 4; index++)
        {
            var image = new Image { image = texture, scaleMode = ScaleMode.ScaleToFit };
            image.style.width = dimension;
            image.style.height = dimension;
            ConfigureMapInteraction(image, texture);
            grid.Add(image);
        }
        imageHost.Add(grid);
    }

    private void BuildComparisonMap(Texture2D before, Texture2D after)
    {
        var row = new VisualElement();
        row.AddToClassList("preview-comparison");
        AddComparisonPane(row, "Before", before);
        AddComparisonPane(row, "After", after);
        imageHost.Add(row);
    }

    private static void AddComparisonPane(VisualElement parent, string title, Texture2D texture)
    {
        var pane = new VisualElement();
        pane.Add(new Label(title));
        var image = new Image { image = texture, scaleMode = ScaleMode.ScaleToFit };
        image.style.width = 220f;
        image.style.height = 220f;
        pane.Add(image);
        parent.Add(pane);
    }

    private void ConfigureMapInteraction(Image image, Texture2D texture)
    {
        image.tooltip = "Drag to pan, double-click to reset, and hover to inspect pixels.";
        image.style.left = mapPan.x;
        image.style.top = mapPan.y;
        image.RegisterCallback<PointerDownEvent>(eventData =>
        {
            if (eventData.button != 0) return;
            if (eventData.clickCount > 1)
            {
                mapPan = Vector2.zero;
                image.style.left = 0f;
                image.style.top = 0f;
                eventData.StopPropagation();
                return;
            }
            panPointerId = eventData.pointerId;
            pointerStart = new Vector2(eventData.position.x, eventData.position.y);
            panStart = mapPan;
            image.CapturePointer(eventData.pointerId);
            eventData.StopPropagation();
        });
        image.RegisterCallback<PointerMoveEvent>(eventData =>
        {
            if (panPointerId == eventData.pointerId && image.HasPointerCapture(eventData.pointerId))
            {
                var position = new Vector2(eventData.position.x, eventData.position.y);
                mapPan = panStart + position - pointerStart;
                image.style.left = mapPan.x;
                image.style.top = mapPan.y;
            }
            ReportPixel(image, texture, eventData.position);
        });
        image.RegisterCallback<PointerUpEvent>(eventData =>
        {
            if (panPointerId != eventData.pointerId) return;
            if (image.HasPointerCapture(eventData.pointerId)) image.ReleasePointer(eventData.pointerId);
            panPointerId = -1;
            eventData.StopPropagation();
        });
    }

    private void ReportPixel(Image image, Texture2D texture, Vector3 worldPosition)
    {
        if (texture == null || !texture.isReadable) return;
        var local = image.WorldToLocal(worldPosition);
        var width = image.contentRect.width;
        var height = image.contentRect.height;
        if (width <= 0f || height <= 0f || local.x < 0f || local.y < 0f || local.x >= width || local.y >= height) return;
        var x = Mathf.Clamp(Mathf.FloorToInt(local.x / width * texture.width), 0, texture.width - 1);
        var y = Mathf.Clamp(texture.height - 1 - Mathf.FloorToInt(local.y / height * texture.height), 0, texture.height - 1);
        var colour = texture.GetPixel(x, y);
        details.text = "Pixel " + x + ", " + y + " • RGBA "
            + colour.r.ToString("0.000") + ", "
            + colour.g.ToString("0.000") + ", "
            + colour.b.ToString("0.000") + ", "
            + colour.a.ToString("0.000") + " • tile " + TileSummary();
    }

    private Texture2D MaskDisplayTexture(Texture2D source)
    {
        if (source == null || string.Equals(activeMaskChannel, "RGBA", StringComparison.Ordinal)) return source;
        if (!source.isReadable)
        {
            details.text = "This cached preview predates CPU pixel inspection. Run Preview once to refresh it.";
            return source;
        }
        if (maskChannelTexture != null && ReferenceEquals(maskChannelSource, source)) return maskChannelTexture;
        DestroyMaskChannelTexture();
        var component = activeMaskChannel.StartsWith("R", StringComparison.Ordinal) ? 0
            : activeMaskChannel.StartsWith("G", StringComparison.Ordinal) ? 1
            : activeMaskChannel.StartsWith("B", StringComparison.Ordinal) ? 2
            : 3;
        var sourcePixels = source.GetPixels32();
        var displayPixels = sourcePixels.Select(pixel =>
        {
            var value = component == 0 ? pixel.r : component == 1 ? pixel.g : component == 2 ? pixel.b : pixel.a;
            return new Color32(value, value, value, byte.MaxValue);
        }).ToArray();
        maskChannelTexture = new Texture2D(source.width, source.height, TextureFormat.RGBA32, false, true)
        {
            name = "Procedural Material Mask " + activeMaskChannel,
            hideFlags = HideFlags.HideAndDontSave,
            wrapMode = TextureWrapMode.Repeat,
            filterMode = FilterMode.Bilinear,
        };
        maskChannelTexture.SetPixels32(displayPixels);
        maskChannelTexture.Apply(false, false);
        maskChannelSource = source;
        return maskChannelTexture;
    }

    private void DestroyMaskChannelTexture()
    {
        if (maskChannelTexture != null) UnityEngine.Object.DestroyImmediate(maskChannelTexture);
        maskChannelTexture = null;
        maskChannelSource = null;
    }

    private PreviewCacheEntry FindCurrentCacheEntry()
    {
        if (document == null) return null;
        var keyPrefix = document.CurrentHash + "|" + previewResolution + "|" + document.SelectedLayerId + "|solo=" + soloLayer;
        foreach (var pair in cache)
        {
            if (pair.Key.StartsWith(keyPrefix, StringComparison.Ordinal)) return pair.Value;
        }
        return null;
    }

    private PreviewCacheEntry LoadPreview(RustTextureBakerClient.BakerResult result, string recipeHash)
    {
        var outputDirectory = result.OutputDirectory;
        if (string.IsNullOrWhiteSpace(outputDirectory))
        {
            outputDirectory = Path.GetFullPath(Path.Combine(Application.dataPath, "..", "Library", "ProceduralMaterialPreview"));
        }
        if (!Directory.Exists(outputDirectory)) return null;
        var maps = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var pair in result.Maps)
        {
            maps[pair.Key] = MakeAbsolute(outputDirectory, pair.Value);
            var lowerName = pair.Key.ToLowerInvariant();
            if (lowerName.Contains("raw_height") || (lowerName.Contains("raw") && lowerName.Contains("height"))) continue;
            if (lowerName.Contains("albedo")) maps["Albedo"] = maps[pair.Key];
            else if (lowerName.Contains("height")) maps["Height"] = maps[pair.Key];
            else if (lowerName.Contains("normal")) maps["Normal"] = maps[pair.Key];
            else if (lowerName.Contains("occlusion") || lowerName == "ao") maps["Occlusion"] = maps[pair.Key];
            else if (lowerName.Contains("mask")) maps["Mask"] = maps[pair.Key];
        }
        foreach (var file in Directory.EnumerateFiles(outputDirectory, "*", SearchOption.AllDirectories))
        {
            var filename = Path.GetFileName(file);
            var extension = Path.GetExtension(file);
            if (string.Equals(extension, ".r16", StringComparison.OrdinalIgnoreCase)
                && filename.IndexOf("preview_height", StringComparison.OrdinalIgnoreCase) >= 0)
            {
                maps["RawHeight"] = file;
                continue;
            }
            if (string.Equals(extension, ".json", StringComparison.OrdinalIgnoreCase)
                && filename.IndexOf("preview_height", StringComparison.OrdinalIgnoreCase) >= 0)
            {
                maps["RawHeightMetadata"] = file;
                continue;
            }
            if (!string.Equals(extension, ".png", StringComparison.OrdinalIgnoreCase)) continue;
            foreach (var tab in tabs.Where(value => value != "Lit"))
            {
                var kind = tab.ToLowerInvariant();
                if (filename.IndexOf(kind, StringComparison.OrdinalIgnoreCase) >= 0 && !maps.ContainsKey(tab)) maps[tab] = file;
            }
            if (document != null && !string.IsNullOrWhiteSpace(document.SelectedLayerId) && filename.IndexOf(document.SelectedLayerId, StringComparison.OrdinalIgnoreCase) >= 0)
            {
                if (filename.IndexOf("raw", StringComparison.OrdinalIgnoreCase) >= 0) maps["LayerRaw"] = file;
                if (filename.IndexOf("remap", StringComparison.OrdinalIgnoreCase) >= 0) maps["LayerRemapped"] = file;
                if (filename.IndexOf("mask", StringComparison.OrdinalIgnoreCase) >= 0) maps["LayerMask"] = file;
            }
        }
        return maps.Count == 0 ? null : new PreviewCacheEntry(maps, outputDirectory);
    }

    private string ResolveMapPath(PreviewCacheEntry entry, string tab)
    {
        if (tab == "Layer")
        {
            if (entry.Maps.TryGetValue("LayerRemapped", out var remapped)) return remapped;
            if (entry.Maps.TryGetValue("LayerRaw", out var raw)) return raw;
            if (entry.Maps.TryGetValue("LayerMask", out var masked)) return masked;
        }
        return entry.Maps.TryGetValue(tab, out var path) ? path : null;
    }

    private void AddToCache(string key, PreviewCacheEntry entry)
    {
        if (cache.TryGetValue(key, out var existing))
        {
            DestroyTextureSet(existing);
            cacheOrder.Remove(key);
        }
        cache[key] = entry;
        cacheOrder.AddLast(key);
        while (cacheOrder.Count > 8)
        {
            var oldest = cacheOrder.First.Value;
            cacheOrder.RemoveFirst();
            if (cache.TryGetValue(oldest, out var oldEntry)) DestroyTextureSet(oldEntry);
            cache.Remove(oldest);
            if (string.Equals(oldest, lastPreviewKey, StringComparison.Ordinal))
            {
                lastPreviewEntry = null;
                lastPreviewKey = string.Empty;
            }
        }
        RefreshTelemetry();
    }

    private bool TryUseCachedPreview()
    {
        if (document == null) return false;
        var key = PreviewCacheKey();
        if (!cache.TryGetValue(key, out var entry)) return false;
        cacheOrder.Remove(key);
        cacheOrder.AddLast(key);
        if (!ReferenceEquals(lastPreviewEntry, entry)) previousPreviewEntry = lastPreviewEntry;
        lastPreviewEntry = entry;
        lastPreviewKey = key;
        lastRecipeHash = document.CurrentHash;
        details.text = "Cached preview " + previewResolution + "×" + previewResolution + " • tile " + TileSummary();
        RefreshTelemetry();
        RefreshDisplay();
        return true;
    }

    private string PreviewCacheKey()
    {
        return document.CurrentHash + "|" + previewResolution + "|" + document.SelectedLayerId + "|solo=" + soloLayer;
    }

    private void RecordTiming(RustTextureBakerClient.BakerResult result)
    {
        if (result?.TimingsMilliseconds == null || result.TimingsMilliseconds.Count == 0) return;
        var summary = string.Join(", ", result.TimingsMilliseconds.Select(pair => pair.Key + " " + pair.Value.ToString("0.0") + " ms"));
        timingHistory.AddFirst(DateTime.Now.ToString("HH:mm:ss") + " • " + summary);
        while (timingHistory.Count > 10) timingHistory.RemoveLast();
        RefreshTelemetry();
    }

    private void RefreshTelemetry()
    {
        if (telemetry == null) return;
        var history = timingHistory.Count == 0 ? "No completed preview timings." : string.Join("\n", timingHistory);
        telemetry.text = "Cache: " + cache.Count + "/8\n" + history;
    }

    private void DestroyTextureSet(PreviewCacheEntry entry)
    {
        if (entry == null) return;
        DestroyMaskChannelTexture();
        if (ReferenceEquals(previousPreviewEntry, entry)) previousPreviewEntry = null;
        if (ReferenceEquals(lastPreviewEntry, entry))
        {
            lastPreviewEntry = null;
            lastPreviewKey = string.Empty;
        }
        entry.Dispose();
        bakerClient.ReleasePreviewDirectory(entry.OutputDirectory);
        RefreshTelemetry();
    }

    private static string MakeAbsolute(string outputDirectory, string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return string.Empty;
        return Path.IsPathRooted(path) ? path : Path.GetFullPath(Path.Combine(outputDirectory, path));
    }

    private string TileSummary()
    {
        var width = document?.Root["physical_tile_width_m"]?.Value<float>() ?? 0f;
        var height = document?.Root["physical_tile_height_m"]?.Value<float>() ?? 0f;
        return width.ToString("0.###") + " × " + height.ToString("0.###") + " m";
    }

    internal static Texture2D LoadReadablePreviewTexture(string path, bool linear = true)
    {
        Texture2D texture = null;
        try
        {
            texture = new Texture2D(2, 2, TextureFormat.RGBA32, false, linear)
            {
                name = "Procedural Material Preview",
                hideFlags = HideFlags.HideAndDontSave,
            };
            // Pixel inspection and mask-channel extraction both read the cached
            // preview on the CPU, so do not discard its readable image data.
            if (!ImageConversion.LoadImage(texture, File.ReadAllBytes(path), false))
            {
                UnityEngine.Object.DestroyImmediate(texture);
                return null;
            }
            texture.wrapMode = TextureWrapMode.Repeat;
            texture.filterMode = FilterMode.Bilinear;
            return texture;
        }
        catch (IOException)
        {
            if (texture != null) UnityEngine.Object.DestroyImmediate(texture);
            return null;
        }
    }

    internal static void FlipTopToBottomRows(byte[] bytes, int width, int height, int bytesPerPixel)
    {
        if (bytes == null || width <= 0 || height <= 1 || bytesPerPixel <= 0) return;
        var rowLength = checked(width * bytesPerPixel);
        if (bytes.Length != checked(rowLength * height)) throw new ArgumentException("Pixel buffer does not match its declared dimensions.", nameof(bytes));
        var row = new byte[rowLength];
        for (var top = 0; top < height / 2; top++)
        {
            var bottom = height - 1 - top;
            Buffer.BlockCopy(bytes, top * rowLength, row, 0, rowLength);
            Buffer.BlockCopy(bytes, bottom * rowLength, bytes, top * rowLength, rowLength);
            Buffer.BlockCopy(row, 0, bytes, bottom * rowLength, rowLength);
        }
    }

    private ProceduralMaterialDocument CreateSoloPreviewDocument(ProceduralMaterialDocument source)
    {
        var copy = ProceduralMaterialDocument.CreateNew();
        var rootCopy = (JObject)source.Root.DeepClone();
        var layers = rootCopy["layers"] as JArray;
        if (layers != null)
        {
            foreach (var token in layers.OfType<JObject>())
            {
                token["enabled"] = string.Equals(token["id"]?.Value<string>(), source.SelectedLayerId, StringComparison.Ordinal);
            }
        }
        copy.SetJson(rootCopy.ToString(Newtonsoft.Json.Formatting.None));
        copy.SelectLayer(source.SelectedLayerId);
        soloPreviewDocument = copy;
        return copy;
    }

    private void DestroySoloPreviewDocument()
    {
        if (soloPreviewDocument == null) return;
        UnityEngine.Object.DestroyImmediate(soloPreviewDocument);
        soloPreviewDocument = null;
    }

    private void DrawLitPreview()
    {
        if (activeTab != "Lit") return;
        var rect = GUILayoutUtility.GetRect(10f, 260f, GUILayout.ExpandWidth(true), GUILayout.ExpandHeight(true));
        if (Event.current.type == EventType.MouseDrag)
        {
            litOrbit += Event.current.delta * 0.6f;
            litPreview.MarkDirtyRepaint();
            Event.current.Use();
        }
        if (Event.current.type == EventType.ScrollWheel)
        {
            litDistance = Mathf.Clamp(litDistance + Event.current.delta.y * 0.03f, 1.2f, 6f);
            litPreview.MarkDirtyRepaint();
            Event.current.Use();
        }
        if (Event.current.type != EventType.Repaint) return;
        var entry = FindCurrentCacheEntry();
        var albedoPath = entry == null ? null : ResolveMapPath(entry, "Albedo");
        if (string.IsNullOrWhiteSpace(albedoPath) || !File.Exists(albedoPath))
        {
            EditorGUI.HelpBox(rect, "Run Preview to populate the lit 3D view. Drag to orbit; scroll to zoom.", MessageType.Info);
            return;
        }
        activeTexture = entry.GetTexture(albedoPath, false);
        EnsureRenderResources();
        previewMaterial.mainTexture = activeTexture;
        var normalPath = ResolveMapPath(entry, "Normal");
        var normal = !string.IsNullOrWhiteSpace(normalPath) && File.Exists(normalPath)
            ? entry.GetTexture(normalPath)
            : null;
        previewMaterial.SetTexture("_BumpMap", normal);
        previewMaterial.SetFloat("_UseNormal", useLitNormal && normal != null ? 1f : 0f);
        // Unity requests DirectX normals from the baker, so the preview does
        // not need a recipe-dependent green-channel conversion.
        previewMaterial.SetFloat("_NormalGreenSign", 1f);
        var occlusionPath = ResolveMapPath(entry, "Occlusion");
        var occlusion = !string.IsNullOrWhiteSpace(occlusionPath) && File.Exists(occlusionPath)
            ? entry.GetTexture(occlusionPath)
            : null;
        previewMaterial.SetTexture("_OcclusionTex", occlusion);
        previewMaterial.SetFloat("_UseOcclusion", useLitOcclusion && occlusion != null ? 1f : 0f);
        previewMaterial.SetFloat("_OcclusionStrength", 1f);
        var height = entry.GetHeightTexture();
        if (useLitHeight && height != null && entry.TryGetHeightRange(out var heightRange))
        {
            var previewingPlane = objectField != null && objectField.value == "Plane";
            var tileWidth = document?.Root["physical_tile_width_m"]?.Value<float>() ?? 1f;
            var tileHeight = document?.Root["physical_tile_height_m"]?.Value<float>() ?? 1f;
            var tileSpan = Mathf.Max(0.001f, Mathf.Max(tileWidth, tileHeight));
            var previewSurfaceSpan = previewingPlane ? 3.6f : Mathf.PI * 0.5f;
            previewMaterial.SetTexture("_HeightTex", height);
            previewMaterial.SetVector("_HeightRange", new Vector4(heightRange.Minimum, heightRange.Maximum, heightRange.Neutral, 0f));
            previewMaterial.SetFloat("_HeightDisplacementScale", previewSurfaceSpan / tileSpan);
            previewMaterial.SetFloat("_UseHeight", 1f);
        }
        else
        {
            previewMaterial.SetTexture("_HeightTex", null);
            previewMaterial.SetFloat("_UseHeight", 0f);
        }
        var mesh = objectField != null && objectField.value == "Plane" ? planeMesh : sphereMesh;
        var matrix = objectField != null && objectField.value == "Plane"
            ? Matrix4x4.TRS(Vector3.zero, Quaternion.Euler(0f, 0f, 0f), Vector3.one * 1.8f)
            : Matrix4x4.identity;
        var lightVector = Quaternion.Euler(0f, lightDirection, 0f) * new Vector3(0.35f, 0.8f, 0.45f).normalized;
        previewMaterial.SetVector("_LightDirection", new Vector4(lightVector.x, lightVector.y, lightVector.z, 0f));
        previewMaterial.SetFloat("_LightStrength", lightStrength);
        renderUtility.BeginPreview(rect, GUIStyle.none);
        renderUtility.lights[0].intensity = lightStrength;
        renderUtility.lights[0].transform.rotation = Quaternion.Euler(35f, lightDirection, 0f);
        var camera = renderUtility.camera;
        camera.clearFlags = CameraClearFlags.Color;
        camera.backgroundColor = new Color(0.11f, 0.11f, 0.11f, 1f);
        camera.nearClipPlane = 0.1f;
        camera.farClipPlane = 100f;
        camera.transform.position = Quaternion.Euler(litOrbit.x, litOrbit.y, 0f) * new Vector3(0f, 0f, -litDistance);
        camera.transform.LookAt(Vector3.zero);
        renderUtility.DrawMesh(mesh, matrix, previewMaterial, 0);
        renderUtility.Render(true);
        renderUtility.EndAndDrawPreview(rect);
    }

    private void EnsureRenderResources()
    {
        if (renderUtility == null)
        {
            renderUtility = new PreviewRenderUtility(true);
            renderUtility.cameraFieldOfView = 30f;
            renderUtility.lights[0].intensity = 1.2f;
            renderUtility.lights[0].transform.rotation = Quaternion.Euler(35f, -35f, 0f);
            renderUtility.lights[1].intensity = 0.5f;
        }
        if (sphereMesh == null) sphereMesh = CreatePrimitiveMesh(PrimitiveType.Sphere);
        if (planeMesh == null) planeMesh = CreatePreviewPlane(64);
        if (previewMaterial == null)
        {
            var shader = Shader.Find("Hidden/ProceduralMaterialStudio/Preview")
                ?? Shader.Find("Standard")
                ?? Shader.Find("Unlit/Texture");
            previewMaterial = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
        }
    }

    private static Mesh CreatePrimitiveMesh(PrimitiveType primitiveType)
    {
        var primitive = GameObject.CreatePrimitive(primitiveType);
        var filter = primitive.GetComponent<MeshFilter>();
        var mesh = UnityEngine.Object.Instantiate(filter.sharedMesh);
        mesh.hideFlags = HideFlags.HideAndDontSave;
        UnityEngine.Object.DestroyImmediate(primitive);
        return mesh;
    }

    private static Mesh CreatePreviewPlane(int subdivisions)
    {
        subdivisions = Mathf.Clamp(subdivisions, 2, 128);
        var side = subdivisions + 1;
        var vertices = new Vector3[side * side];
        var normals = new Vector3[vertices.Length];
        var tangents = new Vector4[vertices.Length];
        var uv = new Vector2[vertices.Length];
        var triangles = new int[subdivisions * subdivisions * 6];
        for (var z = 0; z <= subdivisions; z++)
        {
            for (var x = 0; x <= subdivisions; x++)
            {
                var index = z * side + x;
                var u = x / (float)subdivisions;
                var v = z / (float)subdivisions;
                vertices[index] = new Vector3((u - 0.5f) * 2f, 0f, (v - 0.5f) * 2f);
                normals[index] = Vector3.up;
                tangents[index] = new Vector4(1f, 0f, 0f, 1f);
                uv[index] = new Vector2(u, v);
            }
        }
        var triangleIndex = 0;
        for (var z = 0; z < subdivisions; z++)
        {
            for (var x = 0; x < subdivisions; x++)
            {
                var topLeft = z * side + x;
                var topRight = topLeft + 1;
                var bottomLeft = topLeft + side;
                var bottomRight = bottomLeft + 1;
                triangles[triangleIndex++] = topLeft;
                triangles[triangleIndex++] = bottomLeft;
                triangles[triangleIndex++] = topRight;
                triangles[triangleIndex++] = topRight;
                triangles[triangleIndex++] = bottomLeft;
                triangles[triangleIndex++] = bottomRight;
            }
        }
        var mesh = new Mesh
        {
            name = "Procedural Material Preview Plane",
            hideFlags = HideFlags.HideAndDontSave,
            vertices = vertices,
            normals = normals,
            tangents = tangents,
            uv = uv,
            triangles = triangles,
        };
        mesh.RecalculateBounds();
        return mesh;
    }

    private sealed class PreviewCacheEntry
    {
        private Texture2D heightTexture;
        private bool heightLoaded;
        private HeightRange heightRange;
        private bool heightRangeLoaded;

        internal PreviewCacheEntry(Dictionary<string, string> maps, string outputDirectory)
        {
            Maps = maps;
            OutputDirectory = outputDirectory;
        }
        internal IReadOnlyDictionary<string, string> Maps { get; }
        internal string OutputDirectory { get; }
        private readonly Dictionary<string, Texture2D> textures = new Dictionary<string, Texture2D>(StringComparer.OrdinalIgnoreCase);

        internal Texture2D GetTexture(string path, bool linear = true)
        {
            var cacheKey = path + "|linear=" + linear;
            if (textures.TryGetValue(cacheKey, out var texture)) return texture;
            texture = LoadReadablePreviewTexture(path, linear);
            if (texture != null) textures[cacheKey] = texture;
            return texture;
        }

        internal Texture2D GetHeightTexture()
        {
            if (heightLoaded) return heightTexture;
            heightLoaded = true;
            if (!Maps.TryGetValue("RawHeight", out var rawPath) || !File.Exists(rawPath)) return null;
            if (!TryReadHeightMetadata(out var metadata)) return null;
            try
            {
                var bytes = File.ReadAllBytes(rawPath);
                var expectedLength = checked(metadata.Width * metadata.Height * 2);
                if (bytes.Length != expectedLength) return null;
                if (metadata.BigEndian && BitConverter.IsLittleEndian)
                {
                    for (var index = 0; index < bytes.Length; index += 2)
                    {
                        var first = bytes[index];
                        bytes[index] = bytes[index + 1];
                        bytes[index + 1] = first;
                    }
                }
                if (metadata.TopToBottom) FlipTopToBottomRows(bytes, metadata.Width, metadata.Height, 2);
                heightTexture = new Texture2D(metadata.Width, metadata.Height, TextureFormat.R16, false, true)
                {
                    name = "Procedural Material Preview Height",
                    wrapMode = TextureWrapMode.Repeat,
                    filterMode = FilterMode.Bilinear,
                    hideFlags = HideFlags.HideAndDontSave,
                };
                heightTexture.LoadRawTextureData(bytes);
                heightTexture.Apply(false, true);
                heightRange = metadata.Range;
                heightRangeLoaded = true;
                return heightTexture;
            }
            catch (Exception)
            {
                if (heightTexture != null) UnityEngine.Object.DestroyImmediate(heightTexture);
                heightTexture = null;
                return null;
            }
        }

        internal bool TryGetHeightRange(out HeightRange range)
        {
            if (!heightRangeLoaded) GetHeightTexture();
            range = heightRange;
            return heightRangeLoaded;
        }

        private bool TryReadHeightMetadata(out HeightMetadata metadata)
        {
            metadata = default(HeightMetadata);
            if (!Maps.TryGetValue("RawHeightMetadata", out var metadataPath) || !File.Exists(metadataPath)) return false;
            try
            {
                var value = JObject.Parse(File.ReadAllText(metadataPath));
                var width = value["width"]?.Value<int>() ?? 0;
                var height = value["height"]?.Value<int>() ?? 0;
                var minimum = value["minimum_m"]?.Value<float>() ?? 0f;
                var maximum = value["maximum_m"]?.Value<float>() ?? 0f;
                var neutral = value["base_m"]?.Value<float>() ?? 0f;
                var topToBottom = !string.Equals(value["row_order"]?.Value<string>(), "bottom_to_top", StringComparison.OrdinalIgnoreCase);
                if (width <= 0 || height <= 0 || !IsFinite(minimum) || !IsFinite(maximum) || !IsFinite(neutral) || maximum <= minimum || neutral < minimum || neutral > maximum) return false;
                metadata = new HeightMetadata(width, height, minimum, maximum, neutral, string.Equals(value["endianness"]?.Value<string>(), "big", StringComparison.OrdinalIgnoreCase), topToBottom);
                return true;
            }
            catch (Exception exception) when (exception is IOException || exception is JsonException || exception is FormatException || exception is ArgumentException)
            {
                return false;
            }
        }

        private static bool IsFinite(float value)
        {
            return !float.IsNaN(value) && !float.IsInfinity(value);
        }

        internal void Dispose()
        {
            foreach (var texture in textures.Values) UnityEngine.Object.DestroyImmediate(texture);
            textures.Clear();
            if (heightTexture != null) UnityEngine.Object.DestroyImmediate(heightTexture);
            heightTexture = null;
        }

        private readonly struct HeightMetadata
        {
            internal HeightMetadata(int width, int height, float minimum, float maximum, float neutral, bool bigEndian, bool topToBottom)
            {
                Width = width;
                Height = height;
                Range = new HeightRange(minimum, maximum, neutral);
                BigEndian = bigEndian;
                TopToBottom = topToBottom;
            }

            internal int Width { get; }
            internal int Height { get; }
            internal HeightRange Range { get; }
            internal bool BigEndian { get; }
            internal bool TopToBottom { get; }
        }
    }

    private readonly struct HeightRange
    {
        internal HeightRange(float minimum, float maximum, float neutral)
        {
            Minimum = minimum;
            Maximum = maximum;
            Neutral = neutral;
        }

        internal float Minimum { get; }
        internal float Maximum { get; }
        internal float Neutral { get; }
    }
}
