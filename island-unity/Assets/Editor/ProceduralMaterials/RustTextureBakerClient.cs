using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEditor;
using UnityEngine;
using Debug = UnityEngine.Debug;

/// <summary>
/// Asynchronous process boundary for the engine-neutral Rust baker. All editor
/// commands use temporary recipe snapshots and JSON envelopes; no editor
/// control consumes human progress text as if it were protocol data.
/// </summary>
public sealed class RustTextureBakerClient : IDisposable
{
    private const string ExecutablePreference = "Island.ProceduralMaterialStudio.BakerExecutable";
    private const string CargoPreference = "Island.ProceduralMaterialStudio.UseCargoFallback";
    private const string PreviewRoot = "Library/ProceduralMaterialPreview";

    private PendingRequest pendingPreview;
    private readonly List<PendingRequest> requests = new List<PendingRequest>();
    private readonly List<string> temporaryRecipeSnapshots = new List<string>();
    private readonly HashSet<string> previewDirectories = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
    private long previewGeneration;
    private string pendingPreviewDirectory;
    private bool disposed;

    public bool IsPreviewRunning => pendingPreview != null;
    public string ConfiguredExecutable => EditorPrefs.GetString(ExecutablePreference, string.Empty);
    public bool UseCargoFallback => EditorPrefs.GetBool(CargoPreference, false);

    public void SetConfiguredExecutable(string path)
    {
        EditorPrefs.SetString(ExecutablePreference, path ?? string.Empty);
    }

    public void SetUseCargoFallback(bool value)
    {
        EditorPrefs.SetBool(CargoPreference, value);
    }

    public void RequestSchema(Action<BakerResult> completed)
    {
        Start(RequestKind.Schema, null, null, 0, completed);
    }

    public void RequestValidation(ProceduralMaterialDocument document, Action<BakerResult> completed)
    {
        if (document == null) throw new ArgumentNullException(nameof(document));
        Start(RequestKind.Validate, document, null, 0, completed);
    }

    public void RequestPreview(ProceduralMaterialDocument document, int size, Action<BakerResult> completed)
    {
        if (document == null) throw new ArgumentNullException(nameof(document));
        if (disposed) return;
        CancelPreview();
        var requestGeneration = previewGeneration;
        var outputDirectory = PreviewDirectory(document, size, requestGeneration);
        pendingPreviewDirectory = outputDirectory;
        PendingRequest request = null;
        request = Start(RequestKind.Preview, document, outputDirectory, size, result =>
        {
            var ownsProcess = request == null
                ? string.Equals(pendingPreviewDirectory, outputDirectory, StringComparison.OrdinalIgnoreCase)
                : ReferenceEquals(pendingPreview, request);
            if (requestGeneration != previewGeneration || !ownsProcess)
            {
                ReleasePreviewDirectory(outputDirectory);
                return;
            }
            if (ReferenceEquals(pendingPreview, request)) pendingPreview = null;
            if (string.Equals(pendingPreviewDirectory, outputDirectory, StringComparison.OrdinalIgnoreCase)) pendingPreviewDirectory = null;
            if (result == null || string.Equals(result.Error, "Baker request cancelled.", StringComparison.Ordinal))
            {
                ReleasePreviewDirectory(outputDirectory);
                return;
            }
            if (result != null) result.OutputDirectory = outputDirectory;
            if (!result.Succeeded)
            {
                ReleasePreviewDirectory(outputDirectory);
                completed?.Invoke(result);
                return;
            }
            completed?.Invoke(result);
        }, requestId: requestGeneration);
        pendingPreview = request;
    }

    public void RequestBake(
        ProceduralMaterialDocument document,
        string outputDirectory,
        string profile,
        bool replaceExisting,
        Action<BakerResult> completed)
    {
        if (document == null) throw new ArgumentNullException(nameof(document));
        if (string.IsNullOrWhiteSpace(outputDirectory)) throw new ArgumentException("An output directory is required.", nameof(outputDirectory));
        Start(RequestKind.Bake, document, outputDirectory, 0, result =>
        {
            if (result.Succeeded) result.Profile = profile;
            completed?.Invoke(result);
        }, profile, replaceExisting);
    }

    public void CancelPreview()
    {
        ++previewGeneration;
        var request = pendingPreview;
        var outputDirectory = request?.PreviewOutputDirectory ?? pendingPreviewDirectory;
        pendingPreview = null;
        pendingPreviewDirectory = null;
        request?.Cancel();
        ReleasePreviewDirectory(outputDirectory);
    }

    public void CancelAll()
    {
        ++previewGeneration;
        var outputDirectory = pendingPreviewDirectory;
        PendingRequest[] activeRequests;
        lock (requests) activeRequests = requests.ToArray();
        foreach (var request in activeRequests)
        {
            request.Cancel();
            ReleasePreviewDirectory(request.PreviewOutputDirectory);
        }
        lock (requests) requests.Clear();
        pendingPreview = null;
        pendingPreviewDirectory = null;
        ReleasePreviewDirectory(outputDirectory);
    }

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        CancelAll();
        string[] snapshots;
        lock (temporaryRecipeSnapshots)
        {
            snapshots = temporaryRecipeSnapshots.ToArray();
            temporaryRecipeSnapshots.Clear();
        }
        foreach (var path in snapshots) TryDeleteFile(path);
        string[] directories;
        lock (previewDirectories)
        {
            directories = previewDirectories.ToArray();
            previewDirectories.Clear();
        }
        foreach (var path in directories) TryDeleteDirectory(path);
    }

    /// <summary>
    /// Releases one preview output directory after its decoded maps leave the
    /// bounded in-memory cache. Only directories owned by this client can be
    /// removed.
    /// </summary>
    public void ReleasePreviewDirectory(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return;
        var absolutePath = Path.GetFullPath(path);
        lock (previewDirectories)
        {
            if (!previewDirectories.Contains(absolutePath)) return;
        }
        if (!TryDeleteDirectory(absolutePath)) return;
        lock (previewDirectories)
        {
            previewDirectories.Remove(absolutePath);
        }
    }

    private PendingRequest Start(
        RequestKind kind,
        ProceduralMaterialDocument document,
        string outputDirectory,
        int size,
        Action<BakerResult> completed,
        string profile = null,
        bool replaceExisting = false,
        long requestId = 0)
    {
        if (disposed) return null;
        PendingRequest request = null;
        string recipePath = null;
        try
        {
            var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
            recipePath = document == null ? null : WriteRecipeSnapshot(document, projectRoot);
            if (!string.IsNullOrWhiteSpace(recipePath))
            {
                lock (temporaryRecipeSnapshots) temporaryRecipeSnapshots.Add(recipePath);
            }
            if (!string.IsNullOrWhiteSpace(outputDirectory)) Directory.CreateDirectory(outputDirectory);
            var executable = ResolveExecutable(projectRoot, out var useCargo);
            if (string.IsNullOrWhiteSpace(executable))
            {
                DeliverLater(new BakerResult(kind, 0, false, string.Empty, string.Empty, null, "No island-texture-baker executable was found. Configure a release binary or enable the explicit Cargo fallback."), completed);
                ReleaseRecipeSnapshot(recipePath);
                return null;
            }

            var arguments = BuildArguments(kind, recipePath, outputDirectory, size, profile, replaceExisting, useCargo, projectRoot);
            request = new PendingRequest(this, kind, requestId, document?.EditGeneration ?? 0, recipePath, outputDirectory, completed);
            lock (requests) requests.Add(request);
            request.Start(executable, arguments, projectRoot);
            return request;
        }
        catch (Exception exception)
        {
            if (request != null)
            {
                lock (requests) requests.Remove(request);
            }
            request?.Cancel();
            ReleaseRecipeSnapshot(recipePath);
            DeliverLater(new BakerResult(kind, -1, false, string.Empty, string.Empty, null, exception.Message), completed);
            return null;
        }
    }

    private void OnRequestFinished(PendingRequest request)
    {
        lock (requests) requests.Remove(request);
        ReleaseRecipeSnapshot(request.RecipeSnapshotPath);
        if (disposed) return;
        var result = request.ToResult();
        DeliverLater(result, request.Completed);
    }

    private void DeliverLater(BakerResult result, Action<BakerResult> completed)
    {
        if (completed == null || disposed) return;
        EditorApplication.delayCall += () =>
        {
            if (!disposed) completed(result);
        };
    }

    private string ResolveExecutable(string projectRoot, out bool useCargo)
    {
        useCargo = UseCargoFallback;
        var configured = ConfiguredExecutable;
        if (!useCargo && !string.IsNullOrWhiteSpace(configured))
        {
            if (Path.IsPathRooted(configured) && File.Exists(configured)) return configured;
            if (!Path.IsPathRooted(configured)) return configured;
        }

        var candidates = new[]
        {
            Path.Combine(projectRoot, "..", "island-rs", "target", "release", "island-texture-baker"),
            Path.Combine(projectRoot, "..", "island-rs", "target", "release", "island-texture-baker.exe"),
            Path.Combine(projectRoot, "..", "island-rs", "target", "debug", "island-texture-baker"),
        };
        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate)) return candidate;
        }

        if (useCargo) return "cargo";
        return string.Empty;
    }

    private static string WriteRecipeSnapshot(ProceduralMaterialDocument document, string projectRoot)
    {
        var directory = Path.Combine(projectRoot, PreviewRoot, "recipes");
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, document.CurrentHash + "-" + document.EditGeneration + "-" + Guid.NewGuid().ToString("N") + ".json");
        var temporary = path + ".tmp-" + Guid.NewGuid().ToString("N");
        File.WriteAllText(temporary, document.CurrentJson + Environment.NewLine, new UTF8Encoding(false));
        File.Move(temporary, path);
        return path;
    }

    private string PreviewDirectory(ProceduralMaterialDocument document, int size, long requestId)
    {
        var projectRoot = Path.GetFullPath(Path.Combine(Application.dataPath, ".."));
        var directory = Path.GetFullPath(Path.Combine(projectRoot, PreviewRoot, document.CurrentHash + "-" + document.EditGeneration + "-" + size + "-" + requestId));
        Directory.CreateDirectory(directory);
        lock (previewDirectories) previewDirectories.Add(directory);
        return directory;
    }

    private void ReleaseRecipeSnapshot(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return;
        lock (temporaryRecipeSnapshots)
        {
            if (!temporaryRecipeSnapshots.Remove(path)) return;
        }
        TryDeleteFile(path);
    }

    private static void TryDeleteFile(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static bool TryDeleteDirectory(string path)
    {
        try
        {
            if (Directory.Exists(path)) Directory.Delete(path, true);
            return true;
        }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }
    }

    private static List<string> BuildArguments(RequestKind kind, string recipePath, string outputDirectory, int size, string profile, bool replaceExisting, bool useCargo, string projectRoot)
    {
        var arguments = new List<string>();
        if (useCargo)
        {
            arguments.Add("run");
            arguments.Add("--release");
            arguments.Add("--manifest-path");
            arguments.Add(Path.Combine(projectRoot, "..", "island-rs", "Cargo.toml"));
            arguments.Add("--bin");
            arguments.Add("island-texture-baker");
            arguments.Add("--");
        }
        switch (kind)
        {
            case RequestKind.Schema:
                arguments.Add("schema");
                arguments.Add("--json");
                break;
            case RequestKind.Validate:
                arguments.Add("validate");
                arguments.Add("--recipe");
                arguments.Add(recipePath);
                arguments.Add("--json");
                break;
            case RequestKind.Preview:
                arguments.Add("preview");
                arguments.Add("--recipe");
                arguments.Add(recipePath);
                arguments.Add("--output");
                arguments.Add(outputDirectory);
                arguments.Add("--size");
                arguments.Add(size.ToString());
                arguments.Add("--json");
                break;
            case RequestKind.Bake:
                // Bake deliberately retains the established invocation shape.
                arguments.Add("--recipe");
                arguments.Add(recipePath);
                arguments.Add("--output");
                arguments.Add(outputDirectory);
                if (!string.IsNullOrWhiteSpace(profile))
                {
                    arguments.Add("--profile");
                    arguments.Add(profile);
                }
                if (replaceExisting) arguments.Add("--force");
                break;
        }
        return arguments;
    }

    public enum RequestKind
    {
        Schema,
        Validate,
        Preview,
        Bake,
    }

    public sealed class BakerResult
    {
        internal BakerResult(RequestKind kind, int exitCode, bool succeeded, string standardOutput, string standardError, JObject envelope, string error)
        {
            Kind = kind;
            ExitCode = exitCode;
            Succeeded = succeeded;
            StandardOutput = standardOutput ?? string.Empty;
            StandardError = standardError ?? string.Empty;
            Envelope = envelope;
            Error = error ?? string.Empty;
            Diagnostics = ReadDiagnostics(envelope);
            RecipeHash = envelope?["recipe_hash"]?.Value<string>()
                ?? envelope?["result"]?["recipe_hash"]?.Value<string>()
                ?? envelope?["data"]?["recipe_hash"]?.Value<string>()
                ?? string.Empty;
            OutputDirectory = envelope?["output_directory"]?.Value<string>()
                ?? envelope?["output"]?.Value<string>()
                ?? envelope?["data"]?["output_directory"]?.Value<string>()
                ?? string.Empty;
            Maps = ReadMaps(envelope);
            TimingsMilliseconds = ReadTimings(envelope);
            ManifestPath = envelope?["manifest"]?.Value<string>()
                ?? envelope?["manifest_path"]?.Value<string>()
                ?? envelope?["data"]?["manifest"]?.Value<string>()
                ?? string.Empty;
        }

        public RequestKind Kind { get; }
        public int ExitCode { get; }
        public bool Succeeded { get; }
        public string StandardOutput { get; }
        public string StandardError { get; }
        public JObject Envelope { get; }
        public string Error { get; }
        public string RecipeHash { get; }
        public string OutputDirectory { get; internal set; }
        public string ManifestPath { get; }
        public string Profile { get; internal set; }
        public IReadOnlyList<Diagnostic> Diagnostics { get; }
        public IReadOnlyDictionary<string, string> Maps { get; }
        public IReadOnlyDictionary<string, double> TimingsMilliseconds { get; }

        public string Message
        {
            get
            {
                if (!string.IsNullOrWhiteSpace(Error)) return Error;
                if (!string.IsNullOrWhiteSpace(StandardError)) return StandardError.Trim();
                return "The baker returned an invalid or unsuccessful response.";
            }
        }

        private static IReadOnlyList<Diagnostic> ReadDiagnostics(JObject envelope)
        {
            var result = new List<Diagnostic>();
            var token = envelope?["diagnostics"] ?? envelope?["result"]?["diagnostics"] ?? envelope?["data"]?["diagnostics"];
            if (!(token is JArray diagnostics)) return result;
            foreach (var item in diagnostics.OfType<JObject>())
            {
                result.Add(new Diagnostic(
                    item["pointer"]?.Value<string>() ?? item["path"]?.Value<string>() ?? string.Empty,
                    item["severity"]?.Value<string>() ?? "error",
                    item["code"]?.Value<string>() ?? "BAKER_DIAGNOSTIC",
                    item["message"]?.Value<string>() ?? item.ToString(Formatting.None)));
            }
            return result;
        }

        private static IReadOnlyDictionary<string, string> ReadMaps(JObject envelope)
        {
            var maps = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            var token = envelope?["maps"] ?? envelope?["generated_maps"] ?? envelope?["result"]?["maps"] ?? envelope?["data"]?["maps"];
            if (token is JObject mapObject)
            {
                foreach (var property in mapObject.Properties()) AddClassifiedMap(maps, property.Name, property.Value.Value<string>() ?? string.Empty);
            }
            else if (token is JArray mapArray)
            {
                foreach (var item in mapArray.OfType<JObject>())
                {
                    var name = item["name"]?.Value<string>() ?? item["kind"]?.Value<string>() ?? string.Empty;
                    var path = item["path"]?.Value<string>() ?? item["file"]?.Value<string>() ?? string.Empty;
                    AddClassifiedMap(maps, name, path);
                    var metadata = item["metadata"]?.Value<string>();
                    if (!string.IsNullOrWhiteSpace(metadata)
                        && (string.Equals(ClassifyMap(name, path), "RawHeight", StringComparison.OrdinalIgnoreCase)
                            || Path.GetExtension(path).Equals(".r16", StringComparison.OrdinalIgnoreCase)))
                    {
                        maps["RawHeightMetadata"] = metadata;
                    }
                }
            }
            return maps;
        }

        private static IReadOnlyDictionary<string, double> ReadTimings(JObject envelope)
        {
            var timings = new Dictionary<string, double>(StringComparer.OrdinalIgnoreCase);
            var token = envelope?["timings_ms"]
                ?? envelope?["timings"]
                ?? envelope?["result"]?["timings_ms"]
                ?? envelope?["data"]?["timings_ms"];
            if (!(token is JObject timingObject)) return timings;
            foreach (var property in timingObject.Properties())
            {
                var value = property.Value.Value<double>();
                if (!double.IsNaN(value) && !double.IsInfinity(value) && value >= 0.0)
                {
                    timings[property.Name] = value;
                }
            }
            return timings;
        }

        private static void AddClassifiedMap(IDictionary<string, string> maps, string name, string path)
        {
            if (string.IsNullOrWhiteSpace(path)) return;
            if (!string.IsNullOrWhiteSpace(name)) maps[name] = path;
            var classification = ClassifyMap(name, path);
            if (!string.IsNullOrWhiteSpace(classification))
            {
                maps[classification] = path;
                if (classification.StartsWith("Layer", StringComparison.OrdinalIgnoreCase) && !maps.ContainsKey("Layer")) maps["Layer"] = path;
            }
        }

        private static string ClassifyMap(string name, string path)
        {
            var kind = (name ?? string.Empty).ToLowerInvariant();
            var file = Path.GetFileName(path ?? string.Empty).ToLowerInvariant();
            var stem = Path.GetFileNameWithoutExtension(file);
            var layer = kind.Contains("layer") || file.Contains("_layer_") || file.Contains("layer_");
            if (layer)
            {
                if (kind.Contains("raw") || FileNameHasSuffix(stem, "raw")) return "LayerRaw";
                if (kind.Contains("remap") || FileNameHasSuffix(stem, "remapped") || FileNameHasSuffix(stem, "remap")) return "LayerRemapped";
                if (kind.Contains("mask") || FileNameHasSuffix(stem, "mask")) return "LayerMask";
                return "Layer";
            }
            if (kind.Contains("raw_height") || (file.EndsWith(".r16", StringComparison.Ordinal) && (file.Contains("preview_height") || FileNameHasSuffix(stem, "height")))) return "RawHeight";
            if (kind.Contains("albedo") || FileNameHasSuffix(stem, "albedo")) return "Albedo";
            if (kind.Contains("normal") || FileNameHasSuffix(stem, "normal")) return "Normal";
            if (kind.Contains("occlusion") || kind == "ao" || FileNameHasSuffix(stem, "occlusion") || FileNameHasSuffix(stem, "ao")) return "Occlusion";
            if (kind.Contains("mask") || FileNameHasSuffix(stem, "mask")) return "Mask";
            if (kind.Contains("height") || FileNameHasSuffix(stem, "height")) return "Height";
            return string.Empty;
        }

        private static bool FileNameHasSuffix(string stem, string suffix)
        {
            return string.Equals(stem, suffix, StringComparison.OrdinalIgnoreCase)
                || stem.EndsWith("_" + suffix, StringComparison.OrdinalIgnoreCase)
                || stem.EndsWith("-" + suffix, StringComparison.OrdinalIgnoreCase);
        }
    }

    public sealed class Diagnostic
    {
        internal Diagnostic(string pointer, string severity, string code, string message)
        {
            Pointer = pointer;
            Severity = severity;
            Code = code;
            Message = message;
        }

        public string Pointer { get; }
        public string Severity { get; }
        public string Code { get; }
        public string Message { get; }
        public bool IsError => string.Equals(Severity, "error", StringComparison.OrdinalIgnoreCase);
    }

    private sealed class PendingRequest
    {
        private readonly RustTextureBakerClient owner;
        private readonly object outputLock = new object();
        private readonly StringBuilder standardOutput = new StringBuilder();
        private readonly StringBuilder standardError = new StringBuilder();
        private Process process;
        private int exitCode = -1;
        private bool cancelled;

        internal PendingRequest(RustTextureBakerClient owner, RequestKind kind, long requestId, int generation, string recipeSnapshotPath, string outputDirectory, Action<BakerResult> completed)
        {
            this.owner = owner;
            Kind = kind;
            RequestId = requestId;
            Generation = generation;
            RecipeSnapshotPath = recipeSnapshotPath;
            OutputDirectory = outputDirectory;
            Completed = completed;
        }

        internal RequestKind Kind { get; }
        internal long RequestId { get; }
        internal int Generation { get; }
        internal string RecipeSnapshotPath { get; }
        internal string OutputDirectory { get; }
        internal string PreviewOutputDirectory => Kind == RequestKind.Preview ? OutputDirectory : null;
        internal Action<BakerResult> Completed { get; }

        internal void Start(string executable, IReadOnlyList<string> arguments, string workingDirectory)
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = executable,
                Arguments = JoinArguments(arguments),
                WorkingDirectory = workingDirectory,
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
            };
            process = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
            process.OutputDataReceived += OnOutput;
            process.ErrorDataReceived += OnError;
            process.Exited += OnExited;
            if (!process.Start()) throw new InvalidOperationException("Could not start the island-texture-baker process.");
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
        }

        internal void Cancel()
        {
            cancelled = true;
            try
            {
                if (process != null && !process.HasExited) process.Kill();
            }
            catch (InvalidOperationException) { }
            catch (System.ComponentModel.Win32Exception) { }
            finally
            {
                process?.Dispose();
                process = null;
            }
        }

        internal BakerResult ToResult()
        {
            var stdout = standardOutput.ToString();
            var stderr = standardError.ToString();
            JObject envelope = null;
            string error = cancelled ? "Baker request cancelled." : string.Empty;
            if (!cancelled)
            {
                envelope = ParseEnvelope(stdout);
                if (envelope == null)
                {
                    if (Kind != RequestKind.Bake)
                    {
                        error = "The baker did not return a JSON response envelope.";
                    }
                }
                else if (envelope["success"] == null)
                {
                    error = "The baker response is missing the success field.";
                }
                else if (!envelope["success"].Value<bool>())
                {
                    error = envelope["error"]?.Value<string>() ?? "The baker rejected the request.";
                }
            }
            var succeeded = !cancelled
                && exitCode == 0
                && ((envelope != null && envelope["success"]?.Value<bool>() == true)
                    || (Kind == RequestKind.Bake && envelope == null && string.IsNullOrWhiteSpace(error)));
            return new BakerResult(Kind, exitCode, succeeded, stdout, stderr, envelope, error);
        }

        private void OnOutput(object sender, DataReceivedEventArgs args)
        {
            if (args.Data == null) return;
            lock (outputLock) standardOutput.AppendLine(args.Data);
        }

        private void OnError(object sender, DataReceivedEventArgs args)
        {
            if (args.Data == null) return;
            lock (outputLock) standardError.AppendLine(args.Data);
        }

        private void OnExited(object sender, EventArgs args)
        {
            try { process?.WaitForExit(); }
            catch (InvalidOperationException) { }
            exitCode = process?.ExitCode ?? -1;
            process?.Dispose();
            process = null;
            owner.OnRequestFinished(this);
        }

        private static JObject ParseEnvelope(string stdout)
        {
            if (string.IsNullOrWhiteSpace(stdout)) return null;
            var text = stdout.Trim();
            try
            {
                return JObject.Parse(text);
            }
            catch (JsonException)
            {
                var start = text.IndexOf('{');
                var end = text.LastIndexOf('}');
                if (start < 0 || end <= start) return null;
                try { return JObject.Parse(text.Substring(start, end - start + 1)); }
                catch (JsonException) { return null; }
            }
        }
    }

    private static string JoinArguments(IEnumerable<string> arguments)
    {
        return string.Join(" ", arguments.Select(QuoteArgument));
    }

    private static string QuoteArgument(string value)
    {
        value = value ?? string.Empty;
        if (value.Length > 0 && value.All(character => !char.IsWhiteSpace(character) && character != '\"' && character != '\\')) return value;
        return "\"" + value.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
    }
}
