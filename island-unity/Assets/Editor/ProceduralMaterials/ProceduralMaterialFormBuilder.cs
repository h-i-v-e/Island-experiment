using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEditor;
using UnityEngine.UIElements;

/// <summary>
/// Builds the progressive-disclosure inspector from the Rust schema metadata.
/// Values are written as JSON Pointer patches, so this class owns no recipe
/// DTOs and does not need to know about every future Rust field.
/// </summary>
public sealed class ProceduralMaterialFormBuilder
{
    private readonly ProceduralMaterialSchema schema;

    public ProceduralMaterialFormBuilder(ProceduralMaterialSchema schema)
    {
        this.schema = schema ?? ProceduralMaterialSchema.CreateFallback();
    }

    public VisualElement BuildBaseMaterial(ProceduralMaterialDocument document, Action<string, JToken> onEdit)
    {
        var root = SectionContainer();
        root.Add(new Label("Base material") { name = "base-material-heading" });
        AddString(root, document, "/name", onEdit);
        AddInteger(root, document, "/seed", onEdit);
        AddInteger(root, document, "/width", onEdit);
        AddInteger(root, document, "/height", onEdit);
        AddNumber(root, document, "/physical_tile_width_m", onEdit);
        AddNumber(root, document, "/physical_tile_height_m", onEdit);

        var material = Foldout("Material generator", true);
        AddEnum(material, document, "/material/kind", onEdit, new[] { "layered_noise", "cracked_stone", "rounded_stones" });
        AddObjectLeaves(material, document, "/material", onEdit, new[] { "kind" });
        root.Add(material);

        var output = Foldout("Output and lighting-independent maps", false);
        AddNumber(output, document, "/normal_scale", onEdit);
        AddObjectLeaves(output, document, "/displacement", onEdit, Array.Empty<string>());
        AddObjectLeaves(output, document, "/occlusion", onEdit, new[] { "combine" });
        AddEnum(output, document, "/occlusion/combine/kind", onEdit, new[] { "multiply", "weighted_minimum" });
        AddObjectLeaves(output, document, "/occlusion/combine", onEdit, new[] { "kind" });
        AddObjectLeaves(output, document, "/albedo", onEdit, new[] { "palette", "base_color", "warm_color" });
        AddColourArray(output, document, "/albedo/base_color", onEdit);
        AddColourArray(output, document, "/albedo/warm_color", onEdit);
        AddPalette(output, document, "/albedo/palette", onEdit);
        root.Add(output);
        return root;
    }

    public VisualElement BuildLayer(
        ProceduralMaterialDocument document,
        JObject layer,
        int layerIndex,
        Action<string, JToken> onEdit,
        Action<string> onSelectLayer)
    {
        var root = SectionContainer();
        if (layer == null)
        {
            root.Add(new HelpBox("Select a layer to edit its source and outputs.", HelpBoxMessageType.Info));
            return root;
        }

        var layerPointer = LayerPointer(document, layer, layerIndex);
        var heading = new Label("Editing layer · " + document.GetLayerName(layer, layerIndex));
        heading.AddToClassList("panel-heading");
        root.Add(heading);
        AddString(root, document, layerPointer + "/name", onEdit, "Layer name");
        AddToggle(root, document, layerPointer + "/enabled", onEdit, "Enabled");

        var source = Foldout("Source", true);
        var sourceKind = Read(document, layerPointer + "/source/kind")?.Value<string>();
        AddEnum(source, document, layerPointer + "/source/kind", onEdit, new[]
        {
            "value", "fbm", "billow", "ridged", "cellular_distance", "cellular_distance_to_edge", "cellular_value"
        });
        AddInteger(source, document, layerPointer + "/source/frequency", onEdit);
        if (IsFractalSource(sourceKind))
        {
            AddInteger(source, document, layerPointer + "/source/octaves", onEdit);
            AddNumber(source, document, layerPointer + "/source/lacunarity", onEdit);
            AddNumber(source, document, layerPointer + "/source/gain", onEdit);
        }
        AddNumber(source, document, layerPointer + "/source/offset/0", onEdit, "X offset");
        AddNumber(source, document, layerPointer + "/source/offset/1", onEdit, "Y offset");
        AddInteger(source, document, layerPointer + "/source/seed_domain", onEdit);
        if (IsCellularSource(Read(document, layerPointer + "/source/kind")?.Value<string>()))
        {
            AddNumber(source, document, layerPointer + "/source/cellular_jitter", onEdit, "Cellular jitter", 0.25f);
        }
        AddNullableObjectButton(
            source,
            document,
            layerPointer + "/source/domain_warp",
            onEdit,
            "Add domain warp",
            new JObject
            {
                ["frequency"] = 2,
                ["amplitude"] = 0.15,
                ["octaves"] = 2,
                ["lacunarity"] = 2.0,
                ["gain"] = 0.5,
                ["seed_domain"] = 401,
            });
        if (Read(document, layerPointer + "/source/domain_warp") is JObject warp)
        {
            var warpSection = Foldout("Domain warp", false);
            AddObjectLeaves(warpSection, document, layerPointer + "/source/domain_warp", onEdit, Array.Empty<string>());
            source.Add(warpSection);
        }
        root.Add(source);

        var remap = Foldout("Scalar remap", true);
        AddNumber(remap, document, layerPointer + "/remap/input_min", onEdit);
        AddNumber(remap, document, layerPointer + "/remap/input_max", onEdit);
        AddToggle(remap, document, layerPointer + "/remap/invert", onEdit, "Invert");
        AddNumber(remap, document, layerPointer + "/remap/contrast", onEdit);
        AddNumber(remap, document, layerPointer + "/remap/bias", onEdit);
        AddToggle(remap, document, layerPointer + "/remap/clamp", onEdit, "Clamp");
        AddRemapCurve(remap, document, layerPointer + "/remap/curve", onEdit);
        root.Add(remap);

        var height = Foldout("Height output", true);
        AddToggle(height, document, layerPointer + "/outputs/height/enabled", onEdit, "Enabled");
        AddEnum(height, document, layerPointer + "/outputs/height/blend/kind", onEdit, new[]
        {
            "replace", "add", "subtract", "multiply", "minimum", "maximum", "lerp"
        });
        AddNumber(height, document, layerPointer + "/outputs/height/strength_m", onEdit, "Strength");
        AddNumber(height, document, layerPointer + "/outputs/height/blend/amount", onEdit, "Blend amount");
        root.Add(height);

        var albedo = Foldout("Albedo output", true);
        AddToggle(albedo, document, layerPointer + "/outputs/albedo/enabled", onEdit, "Enabled");
        var albedoBlendPointer = Read(document, layerPointer + "/outputs/albedo/blend")?.Type == JTokenType.String
            ? layerPointer + "/outputs/albedo/blend"
            : layerPointer + "/outputs/albedo/blend/kind";
        AddEnum(albedo, document, albedoBlendPointer, onEdit, new[]
        {
            "replace", "mix", "multiply", "add", "overlay"
        });
        AddNumber(albedo, document, layerPointer + "/outputs/albedo/strength", onEdit);
        AddEnum(albedo, document, layerPointer + "/outputs/albedo/colour_map/kind", onEdit, new[] { "ramp", "gradient" });
        AddColourArray(albedo, document, layerPointer + "/outputs/albedo/colour_map/first", onEdit, "Ramp first");
        AddColourArray(albedo, document, layerPointer + "/outputs/albedo/colour_map/second", onEdit, "Ramp second");
        AddGradientStops(albedo, document, layerPointer + "/outputs/albedo/colour_map/stops", onEdit);
        AddNumber(albedo, document, layerPointer + "/outputs/albedo/hue_influence", onEdit, "Hue influence", 0.0f);
        AddNumber(albedo, document, layerPointer + "/outputs/albedo/saturation_influence", onEdit, "Saturation influence", 0.0f);
        AddNumber(albedo, document, layerPointer + "/outputs/albedo/value_influence", onEdit, "Value influence", 0.0f);
        root.Add(albedo);

        var mask = Foldout("Mask", false);
        var maskToken = Read(document, layerPointer + "/mask");
        if (maskToken == null || maskToken.Type == JTokenType.Null)
        {
            var addMask = new Button(() => onEdit(layerPointer + "/mask", new JObject { ["kind"] = "own" }))
            {
                text = "Add scalar mask",
                tooltip = "Use this layer's remapped scalar as opacity."
            };
            mask.Add(addMask);
        }
        else
        {
            AddEnum(mask, document, layerPointer + "/mask/kind", onEdit, new[] { "own", "noise", "layer" });
            AddEnum(mask, document, layerPointer + "/mask/layer_id", onEdit, EarlierLayerIds(document, layerIndex));
            var maskKind = Read(document, layerPointer + "/mask/kind")?.Value<string>();
            if (string.Equals(maskKind, "noise", StringComparison.Ordinal))
            {
                var maskSource = Foldout("Inline noise mask", false);
                AddEnum(maskSource, document, layerPointer + "/mask/source/kind", onEdit, new[]
                {
                    "value", "fbm", "billow", "ridged", "cellular_distance", "cellular_distance_to_edge", "cellular_value"
                });
                AddInteger(maskSource, document, layerPointer + "/mask/source/frequency", onEdit);
                var maskSourceKind = Read(document, layerPointer + "/mask/source/kind")?.Value<string>();
                if (IsFractalSource(maskSourceKind))
                {
                    AddInteger(maskSource, document, layerPointer + "/mask/source/octaves", onEdit);
                    AddNumber(maskSource, document, layerPointer + "/mask/source/lacunarity", onEdit);
                    AddNumber(maskSource, document, layerPointer + "/mask/source/gain", onEdit);
                }
                AddInteger(maskSource, document, layerPointer + "/mask/source/seed_domain", onEdit);
                if (IsCellularSource(Read(document, layerPointer + "/mask/source/kind")?.Value<string>()))
                {
                    AddNumber(maskSource, document, layerPointer + "/mask/source/cellular_jitter", onEdit, "Cellular jitter", 0.25f);
                }
                mask.Add(maskSource);
            }
            AddToggle(mask, document, layerPointer + "/mask/remap/invert", onEdit, "Invert");
            AddNumber(mask, document, layerPointer + "/mask/remap/input_min", onEdit);
            AddNumber(mask, document, layerPointer + "/mask/remap/input_max", onEdit);
            AddNumber(mask, document, layerPointer + "/mask/remap/contrast", onEdit);
            AddNumber(mask, document, layerPointer + "/mask/remap/bias", onEdit);
            AddToggle(mask, document, layerPointer + "/mask/remap/clamp", onEdit, "Clamp");
            AddRemapCurve(mask, document, layerPointer + "/mask/remap/curve", onEdit);
        }
        root.Add(mask);

        var selected = new HelpBox(
            "Solo preview and before/after comparison are available from the preview pane.",
            HelpBoxMessageType.None);
        selected.AddToClassList("inspector-note");
        root.Add(selected);
        return root;
    }

    private void AddObjectLeaves(VisualElement parent, ProceduralMaterialDocument document, string objectPointer, Action<string, JToken> onEdit, IReadOnlyCollection<string> excluded)
    {
        var value = Read(document, objectPointer) as JObject;
        if (value == null) return;
        foreach (var property in value.Properties())
        {
            if (excluded.Contains(property.Name)) continue;
            var pointer = objectPointer + "/" + Escape(property.Name);
            if (property.Value is JArray array && array.Count == 3 && array.All(item => item.Type == JTokenType.Float || item.Type == JTokenType.Integer))
            {
                AddColourArray(parent, document, pointer, onEdit);
            }
            else if (property.Value is JObject)
            {
                // Nested objects have their own foldout in the relevant form.
            }
            else
            {
                AddToken(parent, document, pointer, onEdit, Humanize(property.Name));
            }
        }
    }

    private void AddNullableObjectButton(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label, JObject defaultValue)
    {
        var token = Read(document, pointer);
        if (token != null && token.Type != JTokenType.Null) return;
        parent.Add(new Button(() => onEdit(pointer, defaultValue.DeepClone())) { text = label });
    }

    private void AddPalette(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit)
    {
        var palette = Read(document, pointer) as JArray;
        if (palette == null) return;
        var foldout = Foldout("Palette", false);
        for (var index = 0; index < palette.Count; index++)
        {
            AddColourArray(foldout, document, pointer + "/" + index, onEdit, "Stop " + (index + 1));
        }
        parent.Add(foldout);
    }

    private void AddGradientStops(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit)
    {
        var stops = Read(document, pointer) as JArray;
        if (stops == null) return;
        var foldout = Foldout("Colour stops", false);
        for (var index = 0; index < stops.Count; index++)
        {
            var stop = stops[index] as JObject;
            if (stop == null) continue;
            AddNumber(foldout, document, pointer + "/" + index + "/position", onEdit, "Stop " + (index + 1));
            AddColourArray(foldout, document, pointer + "/" + index + "/colour", onEdit, "Colour");
        }
        parent.Add(foldout);
    }

    private void AddRemapCurve(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit)
    {
        var points = Read(document, pointer) as JArray;
        if (points == null) return;
        var foldout = Foldout("Remap curve", false);
        for (var index = 0; index < points.Count; index++)
        {
            AddNumber(foldout, document, pointer + "/" + index + "/position", onEdit, "Point " + (index + 1));
            AddNumber(foldout, document, pointer + "/" + index + "/value", onEdit, "Value");
        }
        parent.Add(foldout);
    }

    private void AddColourArray(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label = null)
    {
        var values = Read(document, pointer) as JArray;
        if (values == null || values.Count < 3) return;
        var row = new VisualElement();
        row.AddToClassList("property-row");
        var title = new Label(label ?? Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1)));
        title.AddToClassList("property-label");
        row.Add(title);
        for (var component = 0; component < 3; component++)
        {
            var componentIndex = component;
            var componentLabel = component == 0 ? "R" : component == 1 ? "G" : "B";
            var componentPointer = pointer + "/" + component;
            var metadata = schema.Get(componentPointer);
            var componentValue = values[component].Value<float>();
            var hasRange = TryGetFloatRange(metadata, out var minimum, out var maximum);
            if (!hasRange && componentValue >= 0f && componentValue <= 1f)
            {
                minimum = 0f;
                maximum = 1f;
                hasRange = true;
            }
            if (hasRange)
            {
                var componentRow = new VisualElement();
                var slider = new Slider(componentLabel, minimum, maximum)
                {
                    value = Clamp(componentValue, minimum, maximum),
                    tooltip = "Linear RGB component",
                };
                var precise = new FloatField("Exact")
                {
                    value = slider.value,
                    tooltip = "Linear RGB component",
                };
                slider.RegisterValueChangedCallback(change =>
                {
                    precise.SetValueWithoutNotify(change.newValue);
                    UpdateColourComponent(document, pointer, componentIndex, change.newValue, onEdit);
                });
                precise.RegisterValueChangedCallback(change =>
                {
                    var next = Clamp(change.newValue, minimum, maximum);
                    precise.SetValueWithoutNotify(next);
                    slider.SetValueWithoutNotify(next);
                    UpdateColourComponent(document, pointer, componentIndex, next, onEdit);
                });
                ConfigureField(
                    precise,
                    componentPointer,
                    values[component],
                    metadata,
                    (_, pasted) => UpdateColourComponent(document, pointer, componentIndex, pasted.Value<float>(), onEdit));
                componentRow.Add(slider);
                componentRow.Add(precise);
                row.Add(componentRow);
                continue;
            }

            var field = new FloatField(componentLabel)
            {
                value = componentValue,
                tooltip = "Linear RGB component"
            };
            field.RegisterValueChangedCallback(change =>
            {
                UpdateColourComponent(document, pointer, componentIndex, change.newValue, onEdit);
            });
            ConfigureField(
                field,
                componentPointer,
                values[component],
                metadata,
                (_, pasted) => UpdateColourComponent(document, pointer, componentIndex, pasted.Value<float>(), onEdit));
            row.Add(field);
        }
        parent.Add(row);
    }

    private static void UpdateColourComponent(ProceduralMaterialDocument document, string pointer, int componentIndex, float value, Action<string, JToken> onEdit)
    {
        var updated = (JArray)Read(document, pointer)?.DeepClone() ?? new JArray(0, 0, 0);
        updated[componentIndex] = value;
        onEdit(pointer, updated);
    }

    private void AddToken(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label = null)
    {
        var value = Read(document, pointer);
        if (value == null || value is JObject || value is JArray) return;
        switch (value.Type)
        {
            case JTokenType.Boolean:
                AddToggle(parent, document, pointer, onEdit, label);
                break;
            case JTokenType.Integer:
                AddInteger(parent, document, pointer, onEdit, label);
                break;
            case JTokenType.Float:
                AddNumber(parent, document, pointer, onEdit, label);
                break;
            default:
                AddString(parent, document, pointer, onEdit, label);
                break;
        }
    }

    private void AddString(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label = null)
    {
        var value = Read(document, pointer);
        if (value == null || value.Type == JTokenType.Null) return;
        var metadata = schema.Get(pointer);
        var field = new TextField(schema.Label(pointer, label ?? Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1))))
        {
            value = value.Value<string>() ?? string.Empty,
            tooltip = metadata?.Tooltip ?? string.Empty,
        };
        field.AddToClassList("property-field");
        field.RegisterValueChangedCallback(change => onEdit(pointer, change.newValue));
        ConfigureField(field, pointer, value, metadata, onEdit);
        parent.Add(field);
    }

    private void AddEnum(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, IEnumerable<string> choices)
    {
        var value = Read(document, pointer);
        if (value == null || value.Type == JTokenType.Null) return;
        var metadata = schema.Get(pointer);
        var options = choices?.Where(choice => !string.IsNullOrWhiteSpace(choice)).Distinct().ToList() ?? new List<string>();
        var current = value.Value<string>() ?? string.Empty;
        if (!options.Contains(current)) options.Insert(0, current);
        if (options.Count == 0) { AddString(parent, document, pointer, onEdit); return; }
        var field = new PopupField<string>(schema.Label(pointer, Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1))), options, options.IndexOf(current))
        {
            tooltip = metadata?.Tooltip ?? string.Empty,
        };
        field.RegisterValueChangedCallback(change => onEdit(pointer, change.newValue));
        ConfigureField(field, pointer, value, metadata, onEdit);
        parent.Add(field);
    }

    private void AddNumber(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label = null, JToken defaultValue = null)
    {
        var value = Read(document, pointer);
        if (value == null && defaultValue != null) value = defaultValue;
        if (value == null || (value.Type != JTokenType.Float && value.Type != JTokenType.Integer)) return;
        var metadata = schema.Get(pointer);
        var displayLabel = schema.Label(pointer, label ?? Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1)));
        var numericValue = value.Value<double>();
        if (TryGetFloatRange(metadata, out var minimum, out var maximum))
        {
            var row = new VisualElement();
            row.AddToClassList("property-row");
            var slider = new Slider(displayLabel, minimum, maximum)
            {
                value = Clamp((float)numericValue, minimum, maximum),
                tooltip = Tooltip(metadata),
            };
            var precise = new FloatField("Exact")
            {
                value = slider.value,
                tooltip = Tooltip(metadata),
            };
            slider.RegisterValueChangedCallback(change =>
            {
                precise.SetValueWithoutNotify(change.newValue);
                onEdit(pointer, change.newValue);
            });
            precise.RegisterValueChangedCallback(change =>
            {
                var next = Clamp(change.newValue, minimum, maximum);
                precise.SetValueWithoutNotify(next);
                slider.SetValueWithoutNotify(next);
                onEdit(pointer, next);
            });
            ConfigureField(precise, pointer, value, metadata, onEdit);
            row.Add(slider);
            row.Add(precise);
            parent.Add(row);
            return;
        }

        var field = new FloatField(displayLabel)
        {
            value = (float)numericValue,
            tooltip = Tooltip(metadata),
        };
        field.RegisterValueChangedCallback(change => onEdit(pointer, change.newValue));
        ConfigureField(field, pointer, value, metadata, onEdit);
        parent.Add(field);
    }

    private void AddInteger(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label = null)
    {
        var value = Read(document, pointer);
        if (value == null || value.Type != JTokenType.Integer) return;
        var metadata = schema.Get(pointer);
        var displayLabel = schema.Label(pointer, label ?? Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1)));
        if (IsUnsignedIntegerPointer(pointer))
        {
            AddUnsignedInteger(parent, document, pointer, onEdit, displayLabel, metadata, value);
            return;
        }

        if (!long.TryParse(value.ToString(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var longValue)) return;
        if (longValue < int.MinValue || longValue > int.MaxValue)
        {
            AddIntegerTextField(parent, document, pointer, onEdit, displayLabel, metadata, longValue);
            return;
        }
        if (TryGetIntegerRange(metadata, out var integerMinimum, out var integerMaximum))
        {
            var row = new VisualElement();
            row.AddToClassList("property-row");
            var slider = new SliderInt(displayLabel, integerMinimum, integerMaximum)
            {
                value = Clamp((int)longValue, integerMinimum, integerMaximum),
                tooltip = Tooltip(metadata),
            };
            var precise = new IntegerField("Exact")
            {
                value = slider.value,
                tooltip = Tooltip(metadata),
            };
            slider.RegisterValueChangedCallback(change =>
            {
                precise.SetValueWithoutNotify(change.newValue);
                onEdit(pointer, change.newValue);
            });
            precise.RegisterValueChangedCallback(change =>
            {
                var next = Clamp(change.newValue, integerMinimum, integerMaximum);
                precise.SetValueWithoutNotify(next);
                slider.SetValueWithoutNotify(next);
                onEdit(pointer, next);
            });
            ConfigureField(precise, pointer, value, metadata, onEdit);
            row.Add(slider);
            row.Add(precise);
            parent.Add(row);
            return;
        }

        var field = new IntegerField(schema.Label(pointer, label ?? Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1))))
        {
            value = (int)longValue,
            tooltip = Tooltip(metadata),
        };
        field.RegisterValueChangedCallback(change => onEdit(pointer, change.newValue));
        ConfigureField(field, pointer, value, metadata, onEdit);
        parent.Add(field);
    }

    private void AddUnsignedInteger(
        VisualElement parent,
        ProceduralMaterialDocument document,
        string pointer,
        Action<string, JToken> onEdit,
        string label,
        ProceduralMaterialSchema.PropertyMetadata metadata,
        JToken value)
    {
        if (!ulong.TryParse(value.ToString(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var unsignedValue)) return;
        var field = new TextField(label)
        {
            value = unsignedValue.ToString(CultureInfo.InvariantCulture),
            tooltip = Tooltip(metadata) + " (unsigned 64-bit integer)",
        };
        field.RegisterValueChangedCallback(change =>
        {
            if (ulong.TryParse(change.newValue, NumberStyles.None, CultureInfo.InvariantCulture, out var next)) onEdit(pointer, new JValue(next));
        });
        ConfigureField(field, pointer, value, metadata, onEdit);
        parent.Add(field);
    }

    private void AddIntegerTextField(
        VisualElement parent,
        ProceduralMaterialDocument document,
        string pointer,
        Action<string, JToken> onEdit,
        string label,
        ProceduralMaterialSchema.PropertyMetadata metadata,
        long value)
    {
        var field = new TextField(label)
        {
            value = value.ToString(CultureInfo.InvariantCulture),
            tooltip = Tooltip(metadata),
        };
        field.RegisterValueChangedCallback(change =>
        {
            if (long.TryParse(change.newValue, NumberStyles.Integer, CultureInfo.InvariantCulture, out var next)) onEdit(pointer, new JValue(next));
        });
        ConfigureField(field, pointer, new JValue(value), metadata, onEdit);
        parent.Add(field);
    }

    private static bool TryGetFloatRange(ProceduralMaterialSchema.PropertyMetadata metadata, out float minimum, out float maximum)
    {
        minimum = 0f;
        maximum = 0f;
        if (metadata?.Minimum == null || metadata.Maximum == null) return false;
        var minimumValue = metadata.Minimum.Value;
        var maximumValue = metadata.Maximum.Value;
        if (double.IsNaN(minimumValue)
            || double.IsNaN(maximumValue)
            || double.IsInfinity(minimumValue)
            || double.IsInfinity(maximumValue)
            || minimumValue >= maximumValue
            || maximumValue - minimumValue > 1000000.0) return false;
        minimum = (float)minimumValue;
        maximum = (float)maximumValue;
        return !float.IsNaN(minimum)
            && !float.IsNaN(maximum)
            && !float.IsInfinity(minimum)
            && !float.IsInfinity(maximum)
            && minimum < maximum;
    }

    private static bool TryGetIntegerRange(ProceduralMaterialSchema.PropertyMetadata metadata, out int minimum, out int maximum)
    {
        minimum = 0;
        maximum = 0;
        if (metadata?.Minimum == null || metadata.Maximum == null) return false;
        var minimumValue = metadata.Minimum.Value;
        var maximumValue = metadata.Maximum.Value;
        if (double.IsNaN(minimumValue)
            || double.IsNaN(maximumValue)
            || double.IsInfinity(minimumValue)
            || double.IsInfinity(maximumValue)
            || minimumValue < int.MinValue
            || maximumValue > int.MaxValue
            || minimumValue >= maximumValue
            || maximumValue - minimumValue > 100000.0) return false;
        minimum = (int)minimumValue;
        maximum = (int)maximumValue;
        return minimum < maximum;
    }

    private static float Clamp(float value, float minimum, float maximum)
    {
        return Math.Max(minimum, Math.Min(maximum, value));
    }

    private static int Clamp(int value, int minimum, int maximum)
    {
        return Math.Max(minimum, Math.Min(maximum, value));
    }

    private static bool IsUnsignedIntegerPointer(string pointer)
    {
        return string.Equals(pointer, "/seed", StringComparison.Ordinal)
            || pointer.EndsWith("/seed_domain", StringComparison.Ordinal);
    }

    private void AddToggle(VisualElement parent, ProceduralMaterialDocument document, string pointer, Action<string, JToken> onEdit, string label = null)
    {
        var value = Read(document, pointer);
        if (value == null || value.Type != JTokenType.Boolean) return;
        var metadata = schema.Get(pointer);
        var field = new Toggle(schema.Label(pointer, label ?? Humanize(pointer.Substring(pointer.LastIndexOf('/') + 1))))
        {
            value = value.Value<bool>(),
            tooltip = Tooltip(metadata),
        };
        field.RegisterValueChangedCallback(change => onEdit(pointer, change.newValue));
        ConfigureField(field, pointer, value, metadata, onEdit);
        parent.Add(field);
    }

    private static string LayerPointer(ProceduralMaterialDocument document, JObject layer, int index)
    {
        return "/layers/" + index;
    }

    private static IEnumerable<string> EarlierLayerIds(ProceduralMaterialDocument document, int index)
    {
        var ids = new List<string> { string.Empty };
        var layers = document.Layers;
        if (layers == null) return ids;
        for (var layerIndex = 0; layerIndex < index; layerIndex++)
        {
            if (layers[layerIndex] is JObject layer) ids.Add(document.GetLayerId(layer, layerIndex));
        }
        return ids;
    }

    private static JToken Read(ProceduralMaterialDocument document, string pointer)
    {
        var segments = pointer.Substring(1).Split('/');
        JToken current = document.Root;
        foreach (var segment in segments)
        {
            var key = segment.Replace("~1", "/").Replace("~0", "~");
            if (current is JObject objectCurrent) current = objectCurrent[key];
            else if (current is JArray arrayCurrent && int.TryParse(key, out var index) && index >= 0 && index < arrayCurrent.Count) current = arrayCurrent[index];
            else current = null;
        }
        return current;
    }

    private static VisualElement SectionContainer()
    {
        var section = new VisualElement();
        section.AddToClassList("inspector-section");
        return section;
    }

    private static Foldout Foldout(string text, bool value)
    {
        var foldout = new Foldout { text = text, value = value };
        foldout.AddToClassList("inspector-foldout");
        return foldout;
    }

    private static string Tooltip(ProceduralMaterialSchema.PropertyMetadata metadata)
    {
        if (metadata == null) return string.Empty;
        return string.IsNullOrWhiteSpace(metadata.Unit)
            ? metadata.Tooltip
            : metadata.Tooltip + " (" + metadata.Unit + ")";
    }

    private static void ConfigureField(
        VisualElement field,
        string pointer,
        JToken currentValue,
        ProceduralMaterialSchema.PropertyMetadata metadata,
        Action<string, JToken> onEdit)
    {
        field.userData = pointer;
        field.RegisterCallback<ContextualMenuPopulateEvent>(eventData =>
        {
            if (metadata?.Default != null)
            {
                eventData.menu.AppendAction(
                    "Reset to default",
                    _ => onEdit(pointer, metadata.Default.DeepClone()),
                    DropdownMenuAction.AlwaysEnabled);
            }
            eventData.menu.AppendAction(
                "Copy JSON value",
                _ => EditorGUIUtility.systemCopyBuffer = currentValue?.ToString(Formatting.None) ?? "null",
                DropdownMenuAction.AlwaysEnabled);
            eventData.menu.AppendAction(
                "Paste JSON value",
                _ =>
                {
                    try
                    {
                        onEdit(pointer, JToken.Parse(EditorGUIUtility.systemCopyBuffer));
                    }
                    catch (JsonException error)
                    {
                        UnityEngine.Debug.LogWarning("Clipboard does not contain a valid JSON value: " + error.Message);
                    }
                },
                DropdownMenuAction.AlwaysEnabled);
        });
    }

    private static string Humanize(string value)
    {
        if (string.IsNullOrWhiteSpace(value)) return "Property";
        value = value.Replace('_', ' ');
        return char.ToUpperInvariant(value[0]) + value.Substring(1);
    }

    private static string Escape(string value)
    {
        return (value ?? string.Empty).Replace("~", "~0").Replace("/", "~1");
    }

    private static bool IsCellularSource(string kind)
    {
        return !string.IsNullOrWhiteSpace(kind) && kind.StartsWith("cellular_", StringComparison.Ordinal);
    }

    private static bool IsFractalSource(string kind)
    {
        return string.Equals(kind, "fbm", StringComparison.Ordinal)
            || string.Equals(kind, "billow", StringComparison.Ordinal)
            || string.Equals(kind, "ridged", StringComparison.Ordinal);
    }
}
