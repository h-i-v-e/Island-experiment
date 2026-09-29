using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Threading;
using System.Threading.Tasks;
using Motu.Interop;
using Motu.Islands;
using Motu.Settings;
using UnityEngine;
using UnityEngine.AI;
using UnityEngine.Profiling;
using Debug = UnityEngine.Debug;
using static Motu.Rendering.UnityObjectLifetime;

namespace Motu.Navigation
{
    [Serializable]
    public sealed class IslandNavigationBuildReport
    {
        public int sourceTriangles, sourceCount, agentTypeId;
        public float radius, height, voxelSize;
        public int tileSize, chunkCount, completedChunks, peakChunkTriangles, peakChunkSources, maximumBuildWorkers;
        public float chunkSize;
        public int seamLinks;
        public long peakChunkSourceMeshBytes;
        public bool heightMesh, cancelled;
        public double sourceUploadMilliseconds, bakeMilliseconds, totalBuildMilliseconds, seamMilliseconds, registrationMilliseconds;
        public long sourceMeshBufferBytes, navMeshReportedBytes;
        public long processResidentBaselineBytes, sampledProcessResidentPeakBytes;
        public bool processResidentMemoryAvailable;
        public long unityAllocatedBaselineBytes, sampledUnityAllocatedPeakBytes;
        public string failure;
    }

    [ExecuteAlways]
    public sealed class IslandNavigation : MonoBehaviour, IIslandNavigation
    {
        // Cruise is about 8 m/s and the shore sits near 1 km from the island origin.
        // Starting at 2.5 km leaves a few minutes to bake and skips islands that
        // only pass through the generation radius.
        internal const float ApproachDistanceMetres = 2500f;

        // Bound memory pressure across concurrently prepared islands.
        private static readonly SemaphoreSlim BuildGate = new SemaphoreSlim(1, 1);
        private static readonly SemaphoreSlim ExportGate = new SemaphoreSlim(1, 1);
        private sealed class AgentType { internal int id, users; }
        private static readonly Dictionary<Vector4, AgentType> AgentTypes = new Dictionary<Vector4, AgentType>();
        private readonly CancellationTokenSource lifetime = new CancellationTokenSource();
        private IslandNavigationSources sources;
        private NavMeshData data;
        private sealed class ChunkData
        {
            internal NavMeshData data;
            internal NavMeshDataInstance instance;
            internal Bounds bounds;
            internal Vector2Int key;
        }
        private readonly List<ChunkData> chunks = new List<ChunkData>();
        private readonly List<NavMeshLinkData> seamLinks = new List<NavMeshLinkData>();
        private readonly List<NavMeshLinkInstance> linkInstances = new List<NavMeshLinkInstance>();
        private AsyncOperation operation;
        private IslandNavigationSettings settings;
        private Vector4 agentKey;
        private bool ownsAgentType, building, disposed, registerWhenReady = true;
        private bool approachArmed;
        private NativeIslandHandle approachSource;
        private float approachWorldSize;
        private IslandPreparedMesh approachTerrain;
        private IslandPreparedForestData approachForest;
        private IslandPreparedBoulderCollider[][] approachBoulders;
        private IslandPreparedCaves approachCaves;
        private IslandNavigationSettings approachSettings;
        public bool IsReady { get; private set; }
        public bool IsRegistered => chunks.Count != 0 && chunks.TrueForAll(chunk => chunk.instance.valid);
        public int AgentTypeId { get; private set; } = -1;
        public IslandNavigationBuildReport Report { get; private set; }
        public NavMeshData Data => chunks.Count == 0 ? null : chunks[0].data;
        public int ChunkCount => chunks.Count;
        public Task BuildCompletion { get; private set; }

        internal void StartBuild(IslandPreparedMesh terrain, IslandPreparedForestData forest,
            IslandPreparedBoulderCollider[][] boulders, IslandPreparedCaves caves,
            IslandNavigationSettings configuration)
        {
            if (BuildCompletion != null) throw new InvalidOperationException("Navigation is already scheduled.");
            approachArmed = false;
            BuildCompletion = BuildInBackgroundAsync(terrain, forest, boulders, caves, configuration.Copy());
        }

        internal void ArmApproachBuild(NativeIslandHandle source, float worldSize,
            IslandPreparedForestData forest, IslandPreparedBoulderCollider[][] boulders,
            IslandPreparedCaves caves, IslandNavigationSettings configuration)
            => ArmApproachBuild(null, forest, boulders, caves, configuration, source, worldSize);

        internal void ArmApproachBuild(IslandPreparedMesh terrain, IslandPreparedForestData forest,
            IslandPreparedBoulderCollider[][] boulders, IslandPreparedCaves caves,
            IslandNavigationSettings configuration, NativeIslandHandle source = null, float worldSize = 0f)
        {
            if (configuration == null) throw new ArgumentNullException(nameof(configuration));
            if (BuildCompletion != null || approachArmed)
                throw new InvalidOperationException("Navigation is already scheduled.");
            if (terrain == null)
            {
                if (source == null || !source.IsValid)
                    throw new ArgumentException("Deferred navigation requires a resident native island.", nameof(source));
                if (!(worldSize > 0f))
                    throw new ArgumentOutOfRangeException(nameof(worldSize));
            }
            approachTerrain = terrain;
            approachSource = source;
            approachWorldSize = worldSize;
            approachForest = forest;
            approachBoulders = boulders;
            approachCaves = caves;
            approachSettings = configuration.Copy();
            approachArmed = true;
        }

        public void NoteViewerDistance(float distanceMetres)
        {
            if (!approachArmed || BuildCompletion != null || disposed || !float.IsFinite(distanceMetres)) return;
            if (distanceMetres > ApproachDistanceMetres) return;
            approachArmed = false;
            BuildCompletion = ExportAndBuildAsync();
        }

        private async Task ExportAndBuildAsync()
        {
            try
            {
                await Task.Yield();
                lifetime.Token.ThrowIfCancellationRequested();
                var terrain = approachTerrain;
                if (terrain == null)
                {
                    await ExportGate.WaitAsync(lifetime.Token);
                    try
                    {
                        lifetime.Token.ThrowIfCancellationRequested();
                        if (approachSource == null || !approachSource.IsValid) return;
                        using var lease = approachSource.Acquire();
                        var handle = lease.Value;
                        var worldSize = approachWorldSize;
                        terrain = await Task.Run(
                            () => IslandPreparationPipeline.PrepareNavigationMesh(handle, worldSize),
                            lifetime.Token);
                    }
                    finally { ExportGate.Release(); }
                }
                if (disposed || terrain == null) return;
                await BuildAsync(terrain, approachForest, approachBoulders, approachCaves,
                    approachSettings, lifetime.Token);
            }
            catch (OperationCanceledException) { }
            catch (ObjectDisposedException) { }
            catch (Exception error) { if (!disposed) Debug.LogException(error, this); }
            finally
            {
                approachTerrain = null;
                approachSource = null;
                approachForest = null;
                approachBoulders = null;
                approachCaves = null;
                approachSettings = null;
            }
        }

        private async Task BuildInBackgroundAsync(IslandPreparedMesh terrain, IslandPreparedForestData forest,
            IslandPreparedBoulderCollider[][] boulders, IslandPreparedCaves caves, IslandNavigationSettings configuration)
        {
            // Finish installing visible terrain before uploading navigation sources.
            await Task.Yield();
            if (disposed) return;
            try { await BuildAsync(terrain, forest, boulders, caves, configuration, lifetime.Token); }
            catch (OperationCanceledException) { }
            catch (Exception error) { Debug.LogException(error, this); }
        }

        internal async Task BuildAsync(IslandPreparedMesh terrain, IslandPreparedForestData forest,
            IslandPreparedBoulderCollider[][] boulders, IslandPreparedCaves caves,
            IslandNavigationSettings configuration, CancellationToken cancellation,
            IslandPreparedTreeCollider[][] trunks = null, IslandPreparedTreeCollider[][] logs = null)
        {
            if (building || chunks.Count != 0 || disposed) throw new InvalidOperationException("Navigation already built or disposed.");
            settings = configuration.Copy();
            Report = new IslandNavigationBuildReport();
            if (!settings.Enabled || terrain == null) return;
            building = true;
            using var linked = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token, cancellation);
            var token = linked.Token;
            var gateHeld = false;
            var totalTimer = new Stopwatch();
            try
            {
                await BuildGate.WaitAsync(token);
                gateHeld = true;
                totalTimer.Start();
                token.ThrowIfCancellationRequested();
                if ((transform.lossyScale - Vector3.one).sqrMagnitude > 1e-6f)
                    throw new InvalidOperationException("Island navigation requires unit world scale.");
                // Validate before creating a registered agent type.
                settings.BuildSettings(NavMesh.GetSettingsByIndex(0).agentTypeID);
                agentKey = new Vector4(settings.AgentRadius, settings.AgentHeight, settings.StepHeight, settings.MaximumSlope);
                if (!AgentTypes.TryGetValue(agentKey, out var agentType))
                {
                    agentType = new AgentType { id = NavMesh.CreateSettings().agentTypeID };
                    AgentTypes.Add(agentKey, agentType);
                }
                agentType.users++;
                ownsAgentType = true;
                AgentTypeId = agentType.id;
                var build = settings.BuildSettings(AgentTypeId);
                Report.agentTypeId = AgentTypeId;
                Report.radius = build.agentRadius; Report.height = build.agentHeight;
                Report.voxelSize = build.voxelSize; Report.tileSize = build.tileSize;
                Report.heightMesh = build.buildHeightMesh;
#if ENABLE_IL2CPP && !UNITY_EDITOR
                // IL2CPP cannot query System.Diagnostics process data. Diagnostics
                // must never prevent a bake; zero RSS is explicitly unavailable.
                using Process process = null;
#else
                using var process = Process.GetCurrentProcess();
                Report.processResidentMemoryAvailable = true;
#endif
                SampleMemory(process);
                Report.processResidentBaselineBytes = Report.sampledProcessResidentPeakBytes;
                Report.unityAllocatedBaselineBytes = Report.sampledUnityAllocatedPeakBytes;
                var tileWidth = build.tileSize * build.voxelSize;
                var chunkWidth = Mathf.Max(tileWidth, Mathf.Round(settings.ChunkSize / tileWidth) * tileWidth);
                var padding = build.agentRadius * 2f + build.voxelSize * 3f;
                Report.chunkSize = chunkWidth;
                Report.maximumBuildWorkers = (int)build.maxJobWorkers;
                var partition = await Task.Run(() => IslandNavigationChunks.Prepare(terrain, caves,
                    chunkWidth, padding, settings.MinimumGroundHeight, settings.ExcludeRiverBeds, token), token);
                Report.sourceTriangles = partition.sourceTriangles;
                Report.chunkCount = partition.chunks.Count;
                foreach (var chunk in partition.chunks)
                {
                    token.ThrowIfCancellationRequested();
                    var timer = Stopwatch.StartNew();
                    sources = new IslandNavigationSources(chunk.bounds, padding);
                    foreach (var part in chunk.parts)
                    {
                        sources.AddMesh(part.mesh, part.excludeRiverBeds, part.triangles);
                        await Task.Yield();
                        token.ThrowIfCancellationRequested();
                    }
                    sources.AddForest(forest);
                    sources.AddWood(trunks, false); sources.AddWood(logs, true);
                    sources.AddBoulders(boulders);
                    sources.ExcludeSubmergedGround(settings.MinimumGroundHeight, settings.AgentRadius);
                    Report.sourceUploadMilliseconds += timer.Elapsed.TotalMilliseconds;
                    Report.sourceCount += sources.sources.Count;
                    Report.sourceMeshBufferBytes += sources.meshBytes;
                    Report.peakChunkTriangles = Math.Max(Report.peakChunkTriangles, sources.triangleCount);
                    Report.peakChunkSources = Math.Max(Report.peakChunkSources, sources.sources.Count);
                    Report.peakChunkSourceMeshBytes = Math.Max(Report.peakChunkSourceMeshBytes, sources.meshBytes);
                    data = new NavMeshData(AgentTypeId) { name = $"Island LOD0 navigation {chunk.key.x},{chunk.key.y}" };
                    chunks.Add(new ChunkData { data = data, bounds = chunk.bounds, key = chunk.key });
                    timer.Restart();
                    operation = NavMeshBuilder.UpdateNavMeshDataAsync(data, build, sources.sources, chunk.bounds);
                    while (!operation.isDone)
                    {
                        SampleMemory(process);
                        if (token.IsCancellationRequested) NavMeshBuilder.Cancel(data);
                        await Task.Yield();
                    }
                    Report.bakeMilliseconds += timer.Elapsed.TotalMilliseconds;
                    SampleMemory(process);
                    token.ThrowIfCancellationRequested();
                    Report.navMeshReportedBytes += Profiler.GetRuntimeMemorySizeLong(data);
                    Report.completedChunks++;
                    operation = null;
                    sources.Dispose(); sources = null;
                    timer.Restart();
                    await ConnectChunkAsync(chunk, chunks[chunks.Count - 1], token);
                    Report.seamMilliseconds += timer.Elapsed.TotalMilliseconds;
                    SampleMemory(process);
                    chunk.parts.Clear();
                    // Let deferred mesh destruction finish before the next upload.
                    await Task.Yield();
                }
                token.ThrowIfCancellationRequested();
                IsReady = true;
                var registrationTimer = Stopwatch.StartNew();
                UpdateRegistration();
                Report.registrationMilliseconds = registrationTimer.Elapsed.TotalMilliseconds;
                SampleMemory(process);
                Report.totalBuildMilliseconds = totalTimer.Elapsed.TotalMilliseconds;
                if (settings.LogBuildStatistics)
                    Debug.Log($"Island navigation: {JsonUtility.ToJson(Report)}", this);
            }
            catch (OperationCanceledException) { IsReady = false; Report.cancelled = true; throw; }
            catch (Exception error) { IsReady = false; Report.failure = error.Message; throw; }
            finally
            {
                // Cancellation must complete before releasing native build input.
                if (operation != null && !operation.isDone)
                {
                    NavMeshBuilder.Cancel(data);
                    while (!operation.isDone) await Task.Yield();
                }
                operation = null;
                sources?.Dispose(); sources = null;
                building = false;
                if (!IsReady || disposed) ReleaseData();
                if (gateHeld) BuildGate.Release();
            }
        }

        private async Task ConnectChunkAsync(IslandNavigationChunks.Chunk source, ChunkData current, CancellationToken token)
        {
            for (var axis = 0; axis < 2; axis++)
            {
                var acrossX = axis == 0;
                var neighbourKey = source.key - (acrossX ? Vector2Int.right : Vector2Int.up);
                var neighbour = chunks.Find(chunk => chunk.key == neighbourKey);
                if (neighbour == null) continue;
                var candidates = await Task.Run(() => IslandNavigationSeams.Prepare(source, acrossX, settings, token), token);
                var filter = new NavMeshQueryFilter { agentTypeID = AgentTypeId, areaMask = 1 };
                var voxel = Report.voxelSize;
                var accepted = new List<NavMeshLinkData>();
                // SamplePosition only sees registered data. Keep this neighbour pair
                // registered across slice yields while the island may publish.
                // A dormant or disabled island drops the pair before yielding.
                var neighbourInstance = default(NavMeshDataInstance);
                var currentInstance = default(NavMeshDataInstance);
                try
                {
                    var index = 0;
                    while (index < candidates.Count)
                    {
                        token.ThrowIfCancellationRequested();
                        if (!neighbourInstance.valid)
                        {
                            neighbourInstance = NavMesh.AddNavMeshData(
                                neighbour.data, transform.position, transform.rotation);
                            currentInstance = NavMesh.AddNavMeshData(
                                current.data, transform.position, transform.rotation);
                        }
                        var timer = Stopwatch.StartNew();
                        do
                        {
                            if (IslandNavigationSeams.TryCreate(candidates[index++], acrossX, transform, filter, voxel, out var link)
                                && !IslandNavigationSeams.HasNearbyConnection(link, accepted, transform, filter, voxel))
                            {
                                accepted.Add(link);
                                seamLinks.Add(link);
                            }
                        } while (index < candidates.Count && timer.Elapsed.TotalMilliseconds < 2);
                        if (disposed || !isActiveAndEnabled || !registerWhenReady)
                            ReleaseSeamRegistration(ref neighbourInstance, ref currentInstance);
                        if (index < candidates.Count) await Task.Yield();
                    }
                }
                finally { ReleaseSeamRegistration(ref neighbourInstance, ref currentInstance); }
            }
            Report.seamLinks = seamLinks.Count;
        }

        private static void ReleaseSeamRegistration(ref NavMeshDataInstance neighbour, ref NavMeshDataInstance current)
        {
            if (neighbour.valid) neighbour.Remove();
            if (current.valid) current.Remove();
            neighbour = default;
            current = default;
        }

        private void SampleMemory(Process process)
        {
            if (process != null && Report.processResidentMemoryAvailable)
            {
                try
                {
                    process.Refresh();
                    Report.sampledProcessResidentPeakBytes = Math.Max(Report.sampledProcessResidentPeakBytes, process.WorkingSet64);
                }
                catch (NotSupportedException) { Report.processResidentMemoryAvailable = false; }
                catch (System.ComponentModel.Win32Exception) { Report.processResidentMemoryAvailable = false; }
            }
            Report.sampledUnityAllocatedPeakBytes = Math.Max(Report.sampledUnityAllocatedPeakBytes, Profiler.GetTotalAllocatedMemoryLong());
        }

        // Call while the agent GameObject is inactive, before spawning it on a
        // sampled walkable point. All islands with the same dimensions share an ID.
        public void ConfigureAgent(NavMeshAgent agent)
        {
            if (!IsReady) throw new InvalidOperationException("Island navigation is not ready.");
            if (agent == null) throw new ArgumentNullException(nameof(agent));
            agent.agentTypeID = AgentTypeId;
            agent.radius = settings.AgentRadius;
            agent.height = settings.AgentHeight;
        }

        public void SetRegistered(bool active)
        {
            registerWhenReady = active;
            UpdateRegistration();
        }

        private void UpdateRegistration()
        {
            var wanted = !disposed && IsReady && registerWhenReady && isActiveAndEnabled;
            if (!wanted) RemoveRegistration();
            if (wanted)
                foreach (var chunk in chunks)
                    if (!chunk.instance.valid)
                        chunk.instance = NavMesh.AddNavMeshData(chunk.data, transform.position, transform.rotation);
            if (wanted && linkInstances.Count == 0)
                foreach (var link in seamLinks)
                    linkInstances.Add(NavMesh.AddLink(link, transform.position, transform.rotation));
        }

        private void RemoveRegistration()
        {
            foreach (var link in linkInstances) if (link.valid) link.Remove();
            linkInstances.Clear();
            foreach (var chunk in chunks)
            {
                if (chunk.instance.valid) chunk.instance.Remove();
                chunk.instance = default;
            }
        }

        private void OnEnable() => UpdateRegistration();
        private void OnDisable() { RemoveRegistration(); }
        private void OnDestroy() => Dispose();

        public void Dispose()
        {
            if (disposed) return;
            disposed = true;
            approachArmed = false;
            approachSource = null;
            approachTerrain = null;
            approachForest = null;
            approachBoulders = null;
            approachCaves = null;
            approachSettings = null;
            lifetime.Cancel();
            RemoveRegistration();
            IsReady = false;
            if (operation != null && !operation.isDone) NavMeshBuilder.Cancel(data);
            if (!building) ReleaseData();
        }

        private void ReleaseData()
        {
            RemoveRegistration();
            foreach (var chunk in chunks) DestroyUnityObject(chunk.data);
            chunks.Clear(); seamLinks.Clear(); data = null;
            if (ownsAgentType)
            {
                var type = AgentTypes[agentKey];
                if (--type.users == 0) { NavMesh.RemoveSettings(type.id); AgentTypes.Remove(agentKey); }
                ownsAgentType = false;
            }
            AgentTypeId = -1;
        }
    }
}
