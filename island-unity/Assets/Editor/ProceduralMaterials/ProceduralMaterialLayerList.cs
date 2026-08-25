using System;
using System.Collections.Generic;
using Newtonsoft.Json.Linq;
using UnityEngine.UIElements;

/// <summary>
/// Ordered, draggable layer stack. Stable IDs are used for selection and mask
/// diagnostics; the list never caches a second copy of recipe values.
/// </summary>
public sealed class ProceduralMaterialLayerList
{
    private readonly VisualElement root = new VisualElement();
    private readonly ListView listView = new ListView();
    private readonly ProceduralMaterialDocument document;
    private readonly Action<string> selectLayer;
    private readonly Action<int, int> reorderLayer;
    private readonly Action<string, string> editLayerName;
    private readonly Action<string, bool> editLayerEnabled;
    private readonly List<string> layerIds = new List<string>();

    public ProceduralMaterialLayerList(
        ProceduralMaterialDocument document,
        Action<string> selectLayer,
        Action<int, int> reorderLayer,
        Action<string, string> editLayerName,
        Action<string, bool> editLayerEnabled)
    {
        this.document = document;
        this.selectLayer = selectLayer;
        this.reorderLayer = reorderLayer;
        this.editLayerName = editLayerName;
        this.editLayerEnabled = editLayerEnabled;

        root.AddToClassList("layer-stack");
        var title = new Label("Layers · drag to reorder");
        title.AddToClassList("panel-heading");
        root.Add(title);
        root.Add(new Label("Select a row to edit it in the Selected layer inspector.") { tooltip = "H and A show whether the layer affects height or albedo." });
        listView.name = "procedural-material-layer-list";
        listView.fixedItemHeight = 40;
        listView.reorderable = true;
        listView.reorderMode = ListViewReorderMode.Animated;
        listView.selectionType = SelectionType.Single;
        listView.makeItem = MakeItem;
        listView.bindItem = BindItem;
        listView.selectionChanged += OnSelectionChanged;
        listView.itemIndexChanged += OnItemIndexChanged;
        listView.style.flexGrow = 1;
        listView.style.minHeight = 120f;
        root.Add(listView);
        Rebuild();
    }

    public VisualElement Root => root;

    public void Rebuild()
    {
        layerIds.Clear();
        var layers = document.Layers;
        if (layers != null)
        {
            for (var index = 0; index < layers.Count; index++)
            {
                layerIds.Add(document.GetLayerId(layers[index] as JObject, index));
            }
        }
        listView.itemsSource = layerIds;
        listView.Rebuild();
        var selectedIndex = layerIds.IndexOf(document.SelectedLayerId);
        if (layerIds.Count > 0) listView.SetSelectionWithoutNotify(new[] { selectedIndex >= 0 ? selectedIndex : 0 });
        else listView.SetSelectionWithoutNotify(Array.Empty<int>());
    }

    private VisualElement MakeItem()
    {
        var row = new VisualElement();
        row.AddToClassList("layer-row");
        var enabled = new Toggle { name = "enabled" };
        enabled.AddToClassList("layer-enabled");
        enabled.RegisterValueChangedCallback(change =>
        {
            if (row.userData is LayerRowData data) editLayerEnabled(data.Id, change.newValue);
        });
        row.Add(enabled);

        var name = new TextField { name = "name" };
        name.AddToClassList("layer-name");
        name.RegisterValueChangedCallback(change =>
        {
            if (row.userData is LayerRowData data) editLayerName(data.Id, change.newValue);
        });
        row.Add(name);

        var edit = new Button(() =>
        {
            if (row.userData is LayerRowData data) selectLayer(data.Id);
        }) { name = "edit", text = "Edit" };
        edit.AddToClassList("layer-edit");
        row.Add(edit);

        var height = new Label("H") { name = "height-badge" };
        height.AddToClassList("layer-badge");
        row.Add(height);
        var albedo = new Label("A") { name = "albedo-badge" };
        albedo.AddToClassList("layer-badge");
        row.Add(albedo);
        var warning = new Label("!") { name = "warning-badge", tooltip = "Layer has an unresolved mask reference." };
        warning.AddToClassList("layer-warning");
        row.Add(warning);
        return row;
    }

    private void BindItem(VisualElement element, int index)
    {
        var layers = document.Layers;
        if (layers == null || index < 0 || index >= layers.Count) return;
        var layer = layers[index] as JObject;
        var id = document.GetLayerId(layer, index);
        element.userData = new LayerRowData(id);
        var enabled = element.Q<Toggle>("enabled");
        enabled?.SetValueWithoutNotify(layer?["enabled"]?.Value<bool>() ?? true);
        var name = element.Q<TextField>("name");
        name?.SetValueWithoutNotify(document.GetLayerName(layer, index));
        var height = element.Q<Label>("height-badge");
        var albedo = element.Q<Label>("albedo-badge");
        height?.EnableInClassList("layer-badge-active", IsEnabled(layer, "height"));
        albedo?.EnableInClassList("layer-badge-active", IsEnabled(layer, "albedo"));
        var warning = element.Q<Label>("warning-badge");
        warning?.EnableInClassList("layer-warning-visible", HasWarning(layer, index));
    }

    private void OnSelectionChanged(IEnumerable<object> selection)
    {
        foreach (var selected in selection)
        {
            var index = layerIds.IndexOf(selected as string);
            if (index >= 0)
            {
                selectLayer(layerIds[index]);
                return;
            }
        }
    }

    private void OnItemIndexChanged(int from, int to)
    {
        if (from >= 0 && to >= 0 && from != to) reorderLayer(from, to);
    }

    private bool HasWarning(JObject layer, int index)
    {
        var mask = layer?["mask"] as JObject;
        if (!string.Equals(mask?["kind"]?.Value<string>(), "layer", StringComparison.Ordinal)) return false;
        var reference = mask["layer_id"]?.Value<string>();
        var layers = document.Layers;
        if (layers == null || string.IsNullOrWhiteSpace(reference)) return true;
        for (var earlier = 0; earlier < index; earlier++)
        {
            if (string.Equals(document.GetLayerId(layers[earlier] as JObject, earlier), reference, StringComparison.Ordinal)) return false;
        }
        return true;
    }

    private static bool IsEnabled(JObject layer, string output)
    {
        return layer?["outputs"]?[output]?["enabled"]?.Value<bool>() ?? false;
    }

    private sealed class LayerRowData
    {
        internal LayerRowData(string id) { Id = id; }
        internal string Id { get; }
    }
}
