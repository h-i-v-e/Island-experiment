using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEditor;
using UnityEngine;

/// <summary>
/// Loss-resistant editor document for one current Rust texture recipe.
///
/// The editor deliberately stores the JSON DOM instead of mirroring Rust's
/// serde types in C#. Unknown properties therefore survive an edit/save cycle,
/// while the Rust baker remains the authority for schema and validation.
/// </summary>
public sealed class ProceduralMaterialDocument : ScriptableObject, ISerializationCallbackReceiver
{
    [SerializeField] private string sourceFilePath = string.Empty;
    [SerializeField] private string originalFileHash = string.Empty;
    [SerializeField] private string currentJson = "{}";
    [SerializeField] private string schemaMetadataJson = string.Empty;
    [SerializeField] private string selectedLayerId = string.Empty;
    [SerializeField] private string lastBakeHash = string.Empty;
    [SerializeField] private bool dirty;
    [SerializeField] private int editGeneration;

    [NonSerialized] private JObject root;

    public event Action<ProceduralMaterialDocument> Changed;

    public string SourceFilePath => sourceFilePath;
    public string OriginalFileHash => originalFileHash;
    public string CurrentJson => currentJson;
    public string SchemaMetadataJson => schemaMetadataJson;
    public string SelectedLayerId => selectedLayerId;
    public string LastBakeHash => lastBakeHash;
    public bool IsDirty => dirty;
    public int EditGeneration => editGeneration;
    public string CurrentHash => HashJson(currentJson);

    public void OnBeforeSerialize()
    {
        if (root != null) currentJson = root.ToString(Formatting.Indented);
    }

    public void OnAfterDeserialize()
    {
        root = null;
    }

    public JObject Root
    {
        get
        {
            EnsureRoot();
            return root;
        }
    }

    public static ProceduralMaterialDocument CreateNew()
    {
        var document = CreateInstance<ProceduralMaterialDocument>();
        document.sourceFilePath = string.Empty;
        document.originalFileHash = string.Empty;
        document.schemaMetadataJson = string.Empty;
        document.selectedLayerId = "broad-variation";
        document.lastBakeHash = string.Empty;
        document.dirty = true;
        document.editGeneration = 0;
        document.SetJsonInternal(CreateDefaultRecipe(), false);
        return document;
    }

    public static ProceduralMaterialDocument Load(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) throw new ArgumentException("A recipe path is required.", nameof(path));
        var absolutePath = Path.GetFullPath(path);
        var text = File.ReadAllText(absolutePath, Encoding.UTF8);
        var document = CreateInstance<ProceduralMaterialDocument>();
        document.sourceFilePath = absolutePath;
        document.originalFileHash = HashBytes(File.ReadAllBytes(absolutePath));
        document.schemaMetadataJson = string.Empty;
        document.selectedLayerId = string.Empty;
        document.lastBakeHash = string.Empty;
        document.dirty = false;
        document.editGeneration = 0;
        document.SetJsonInternal(ParseObject(text), false);
        document.SelectFirstLayerIfNeeded();
        return document;
    }

    public void SetSchemaMetadata(string json)
    {
        schemaMetadataJson = json ?? string.Empty;
        Changed?.Invoke(this);
    }

    public void SetLastBakeHash(string hash)
    {
        lastBakeHash = hash ?? string.Empty;
        EditorUtility.SetDirty(this);
    }

    public void SelectLayer(string layerId)
    {
        selectedLayerId = layerId ?? string.Empty;
        SelectFirstLayerIfNeeded();
        EditorUtility.SetDirty(this);
        Changed?.Invoke(this);
    }

    public JArray Layers
    {
        get
        {
            return Root["layers"] as JArray;
        }
    }

    public JObject SelectedLayer
    {
        get
        {
            var layers = Layers;
            if (layers == null) return null;
            for (var index = 0; index < layers.Count; index++)
            {
                if (string.Equals(GetLayerId((JObject)layers[index], index), selectedLayerId, StringComparison.Ordinal))
                {
                    return layers[index] as JObject;
                }
            }
            return layers.Count == 0 ? null : layers[0] as JObject;
        }
    }

    public string GetLayerId(JObject layer, int index)
    {
        if (layer == null) return string.Empty;
        var id = layer["id"]?.Value<string>();
        if (!string.IsNullOrWhiteSpace(id)) return id;
        return "layer-" + index;
    }

    public string GetLayerName(JObject layer, int index)
    {
        if (layer == null) return "Layer " + (index + 1);
        var name = layer["name"]?.Value<string>();
        return string.IsNullOrWhiteSpace(name) ? GetLayerId(layer, index) : name;
    }

    public bool TrySet(string pointer, JToken value, string undoLabel = "Edit procedural material")
    {
        if (string.IsNullOrWhiteSpace(pointer) || pointer[0] != '/') return false;
        EnsureRoot();
        var segments = ParsePointer(pointer);
        if (segments.Length == 0) return false;
        JToken parent = root;
        for (var index = 0; index < segments.Length - 1; index++)
        {
            var next = Child(parent, segments[index]);
            if (next == null)
            {
                var created = IsArrayIndex(segments[index + 1]) ? (JToken)new JArray() : new JObject();
                parent[segments[index]] = created;
                next = created;
            }
            parent = next;
        }

        var leaf = segments[segments.Length - 1];
        if (parent is JObject objectParent)
        {
            objectParent[leaf] = value?.DeepClone() ?? JValue.CreateNull();
        }
        else if (parent is JArray arrayParent && int.TryParse(leaf, out var arrayIndex))
        {
            while (arrayParent.Count <= arrayIndex) arrayParent.Add(JValue.CreateNull());
            arrayParent[arrayIndex] = value?.DeepClone() ?? JValue.CreateNull();
        }
        else
        {
            return false;
        }

        CommitEdit();
        return true;
    }

    public bool TryRemove(string pointer)
    {
        if (string.IsNullOrWhiteSpace(pointer) || pointer[0] != '/') return false;
        EnsureRoot();
        var segments = ParsePointer(pointer);
        if (segments.Length == 0) return false;
        var parent = segments.Length == 1
            ? (JToken)root
            : root.SelectToken(ToSelectPath(segments, segments.Length - 1));
        if (parent == null) return false;
        var leaf = segments[segments.Length - 1];
        var removed = parent is JObject objectParent
            ? objectParent.Remove(leaf)
            : parent is JArray arrayParent
                && int.TryParse(leaf, out var arrayIndex)
                && arrayIndex >= 0
                && arrayIndex < arrayParent.Count
                && RemoveAt(arrayParent, arrayIndex);
        if (!removed) return false;
        CommitEdit();
        return true;
    }

    public void SetJson(string json, string undoLabel = "Edit procedural material")
    {
        var parsed = ParseObject(json);
        SetJsonInternal(parsed, true);
        if (!string.IsNullOrWhiteSpace(selectedLayerId)) SelectFirstLayerIfNeeded();
        Changed?.Invoke(this);
    }

    /// <summary>
    /// Rebuilds the non-serialized JSON DOM after Unity restores a serialized
    /// ScriptableObject through Undo/Redo.
    /// </summary>
    public void RehydrateJson()
    {
        root = ParseObject(currentJson);
        SelectFirstLayerIfNeeded();
        EditorUtility.SetDirty(this);
        Changed?.Invoke(this);
    }

    public SaveResult Save(bool overwriteExternalChanges)
    {
        if (string.IsNullOrWhiteSpace(sourceFilePath)) return SaveResult.SaveAsRequired;
        return SaveToPath(sourceFilePath, overwriteExternalChanges, updateSource: true);
    }

    public SaveResult SaveAs(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return SaveResult.Cancelled;
        var absolutePath = Path.GetFullPath(path);
        return SaveToPath(absolutePath, true, updateSource: true);
    }

    public SaveResult SaveCopy(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return SaveResult.Cancelled;
        return SaveToPath(Path.GetFullPath(path), true, updateSource: false);
    }

    public void MarkCleanFromDisk()
    {
        if (!string.IsNullOrWhiteSpace(sourceFilePath) && File.Exists(sourceFilePath))
        {
            originalFileHash = HashBytes(File.ReadAllBytes(sourceFilePath));
        }
        dirty = false;
        EditorUtility.SetDirty(this);
        Changed?.Invoke(this);
    }

    public bool HasExternalChanges()
    {
        if (string.IsNullOrWhiteSpace(sourceFilePath)) return false;
        if (!File.Exists(sourceFilePath)) return !string.IsNullOrWhiteSpace(originalFileHash);
        return !string.Equals(
            originalFileHash,
            HashBytes(File.ReadAllBytes(sourceFilePath)),
            StringComparison.OrdinalIgnoreCase);
    }

    public void ReplaceFromDisk()
    {
        if (string.IsNullOrWhiteSpace(sourceFilePath) || !File.Exists(sourceFilePath)) return;
        var loaded = Load(sourceFilePath);
        sourceFilePath = loaded.sourceFilePath;
        originalFileHash = loaded.originalFileHash;
        currentJson = loaded.currentJson;
        schemaMetadataJson = loaded.schemaMetadataJson;
        selectedLayerId = loaded.selectedLayerId;
        lastBakeHash = loaded.lastBakeHash;
        dirty = false;
        editGeneration++;
        root = loaded.root;
        DestroyImmediate(loaded);
        EditorUtility.SetDirty(this);
        Changed?.Invoke(this);
    }

    public static string HashJson(string json)
    {
        try
        {
            var token = JToken.Parse(json ?? "{}");
            return HashBytes(Encoding.UTF8.GetBytes(token.ToString(Formatting.None)));
        }
        catch (JsonException)
        {
            return HashBytes(Encoding.UTF8.GetBytes(json ?? string.Empty));
        }
    }

    public static string HashBytes(byte[] bytes)
    {
        using (var sha = SHA256.Create())
        {
            var hash = sha.ComputeHash(bytes ?? Array.Empty<byte>());
            var builder = new StringBuilder(hash.Length * 2);
            foreach (var value in hash) builder.Append(value.ToString("x2"));
            return builder.ToString();
        }
    }

    private void EnsureRoot()
    {
        if (root != null) return;
        root = ParseObject(currentJson);
    }

    private void SetJsonInternal(JObject value, bool markEdited)
    {
        root = value ?? new JObject();
        currentJson = root.ToString(Formatting.Indented);
        if (markEdited) CommitEdit();
    }

    private void CommitEdit()
    {
        currentJson = root.ToString(Formatting.Indented);
        dirty = true;
        editGeneration++;
        EditorUtility.SetDirty(this);
        Changed?.Invoke(this);
    }

    private SaveResult SaveToPath(string path, bool overwriteExternalChanges, bool updateSource)
    {
        EnsureRoot();
        if (!overwriteExternalChanges && updateSource && HasExternalChanges())
        {
            return SaveResult.ExternalChanges;
        }

        var directory = Path.GetDirectoryName(path);
        if (string.IsNullOrWhiteSpace(directory)) return SaveResult.Failed;
        var temporaryPath = path + ".tmp-" + Guid.NewGuid().ToString("N");
        try
        {
            Directory.CreateDirectory(directory);
            var serialized = root.ToString(Formatting.Indented) + Environment.NewLine;
            var bytes = new UTF8Encoding(false).GetBytes(serialized);
            File.WriteAllBytes(temporaryPath, bytes);
            ReplaceFileAtomically(temporaryPath, path);
            if (updateSource)
            {
                sourceFilePath = path;
                originalFileHash = HashBytes(bytes);
                dirty = false;
            }
        }
        catch (Exception exception)
        {
            Debug.LogException(exception);
            if (File.Exists(temporaryPath))
            {
                Debug.LogError("The failed procedural material save was preserved for recovery at " + temporaryPath + ".");
            }
            return SaveResult.Failed;
        }

        if (updateSource)
        {
            try
            {
                EditorUtility.SetDirty(this);
                Changed?.Invoke(this);
            }
            catch (Exception exception)
            {
                Debug.LogException(exception);
            }
        }
        return SaveResult.Succeeded;
    }

    private static void ReplaceFileAtomically(string temporaryPath, string destinationPath)
    {
        if (!File.Exists(destinationPath))
        {
            File.Move(temporaryPath, destinationPath);
            return;
        }
        // File.Replace is the only supported path when a destination exists.
        // Never delete the destination and move the temp file: that fallback
        // creates a window where a failed save can destroy the last good recipe.
        // If replacement is unavailable or fails, the caller preserves the temp
        // file as a recovery artifact and leaves the destination untouched.
        File.Replace(temporaryPath, destinationPath, null);
    }

    private void SelectFirstLayerIfNeeded()
    {
        var layers = Layers;
        if (layers == null || layers.Count == 0)
        {
            selectedLayerId = string.Empty;
            return;
        }
        for (var index = 0; index < layers.Count; index++)
        {
            if (string.Equals(GetLayerId(layers[index] as JObject, index), selectedLayerId, StringComparison.Ordinal)) return;
        }
        selectedLayerId = GetLayerId(layers[0] as JObject, 0);
    }

    private static JObject ParseObject(string json)
    {
        var token = JToken.Parse(string.IsNullOrWhiteSpace(json) ? "{}" : json);
        if (!(token is JObject value)) throw new JsonException("A procedural material recipe must be a JSON object.");
        return value;
    }

    private static string[] ParsePointer(string pointer)
    {
        var raw = pointer.Substring(1).Split('/');
        for (var index = 0; index < raw.Length; index++)
        {
            raw[index] = raw[index].Replace("~1", "/").Replace("~0", "~");
        }
        return raw;
    }

    private static string ToSelectPath(string[] segments, int count)
    {
        var builder = new StringBuilder();
        for (var index = 0; index < count; index++)
        {
            if (index > 0) builder.Append('.');
            builder.Append(segments[index].Replace("'", "''"));
        }
        return builder.ToString();
    }

    private static bool IsArrayIndex(string segment)
    {
        return int.TryParse(segment, out _);
    }

    private static JToken Child(JToken parent, string segment)
    {
        if (parent is JObject objectParent) return objectParent[segment];
        if (parent is JArray arrayParent && int.TryParse(segment, out var index) && index >= 0 && index < arrayParent.Count) return arrayParent[index];
        return null;
    }

    private static bool RemoveAt(JArray array, int index)
    {
        array.RemoveAt(index);
        return true;
    }

    private static JObject CreateDefaultRecipe()
    {
        return JObject.Parse(@"{
  'name': 'NewProceduralMaterial',
  'seed': 1,
  'width': 2048,
  'height': 2048,
  'physical_tile_width_m': 2.0,
  'physical_tile_height_m': 2.0,
  'material': {
    'kind': 'cracked_stone',
    'cells_x': 8,
    'cells_y': 8,
    'cell_jitter': 0.25,
    'warp_amplitude': 0.16,
    'crack_width': 0.035,
    'shoulder_width': 0.18,
    'crack_depth': 0.13,
    'slab_variation': 0.035,
    'fracture_probability': 0.28,
    'fracture_depth': 0.045,
    'surface_amplitude': 0.014,
    'broad_variation': 0.018
  },
  'layers': [
    {
      'id': 'broad-variation',
      'name': 'Broad variation',
      'enabled': true,
      'source': {
        'kind': 'fbm',
        'frequency': 3,
        'octaves': 4,
        'lacunarity': 2.0,
        'gain': 0.5,
        'offset': [0.0, 0.0],
        'seed_domain': 101,
        'domain_warp': null
      },
      'remap': {
        'input_min': -1.0,
        'input_max': 1.0,
        'invert': false,
        'contrast': 1.0,
        'bias': 0.0,
        'clamp': true
      },
      'mask': null,
      'outputs': {
        'height': {
          'enabled': true,
          'blend': { 'kind': 'add' },
          'strength_m': 0.012
        },
          'albedo': {
          'enabled': false,
          'blend': 'mix',
          'strength': 0.3,
          'colour_map': {
            'kind': 'gradient',
            'stops': [
              { 'position': 0.0, 'colour': [0.22, 0.24, 0.20] },
              { 'position': 1.0, 'colour': [0.42, 0.36, 0.28] }
            ]
          }
        }
      }
    }
  ],
  'normal_convention': 'open_gl',
  'normal_scale': 1.0,
  'displacement': { 'minimum_m': -0.2, 'maximum_m': 0.2, 'base_m': 0.0, 'displacement_map': true },
  'occlusion': { 'directions': 8, 'samples': 6, 'radius': 1.0, 'max_radius': 8.0, 'cavity_strength': 1.5, 'horizon_strength': 0.85, 'power': 1.0, 'combine': { 'kind': 'multiply' } },
  'albedo': { 'base_color': [0.25, 0.27, 0.24], 'warm_color': [0.42, 0.36, 0.28], 'palette': [[0.25, 0.27, 0.24], [0.34, 0.34, 0.30], [0.42, 0.36, 0.28]], 'variation': 0.58, 'crack_darkening': 0.48, 'shoulder_variation': 0.06, 'mineral_density': 0.0, 'mineral_brightness': 0.12, 'occlusion_influence': 0.08 },
  'output_profiles': ['separate', 'motu_unity_terrain']
}");
    }

    public enum SaveResult
    {
        Succeeded,
        SaveAsRequired,
        ExternalChanges,
        Cancelled,
        Failed,
    }
}
