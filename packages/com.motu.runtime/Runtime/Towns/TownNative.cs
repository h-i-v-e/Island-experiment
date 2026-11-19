using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.IO;
using UnityEngine;

namespace Motu.Towns
{
    internal static class TownNative
    {
        internal const int ApiVersion = 2;
        private const int RoadGeometryRejected = -1;

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetCoastalTownApiVersion();
        [DllImport("motu_building", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint GetMotuTownApiVersion();
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr ExportCoastalTownInputJson(IntPtr island, ulong tileSeed);
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern void FreeCoastalTownJson(IntPtr json);
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern int ApplyCoastalTownRoadsJson(IntPtr island, [MarshalAs(UnmanagedType.LPUTF8Str)] string json);
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern int SetCoastalTownClearanceJson(IntPtr island, [MarshalAs(UnmanagedType.LPUTF8Str)] string json);
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr PrepareCoastalCartTracks(IntPtr island, [MarshalAs(UnmanagedType.LPUTF8Str)] string json);
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr GetLastCoastalTownError();
        [DllImport("motu_building", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr CreateMotuTown([MarshalAs(UnmanagedType.LPUTF8Str)] string request,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string basePath);
        [DllImport("motu_building", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr ExportMotuTownJson(IntPtr town);
        [DllImport("motu_building", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr GetLastMotuTownError();
        [DllImport("motu_building", CallingConvention = CallingConvention.Cdecl)]
        private static extern void DestroyMotuTown(IntPtr town);

        internal static TownPreparedData Prepare(IntPtr island, string recipeJson, ulong tileSeed,
            TownDependencyData[] dependencies = null, string villageRecipeJson = null,
            bool connectSettlements = true, bool connectCoastalSeams = false)
        {
            if (string.IsNullOrEmpty(recipeJson)) return null;
            if (GetCoastalTownApiVersion() != ApiVersion)
                throw new InvalidOperationException("The native coastal town libraries do not match this package. Install the matching libraries and restart Unity.");
            var input = ExportCoastalTownInputJson(island, tileSeed);
            if (input == IntPtr.Zero) throw Error("Coastal town terrain export", GetLastCoastalTownError());
            string inputJson;
            try { inputJson = Marshal.PtrToStringUTF8(input); }
            finally { FreeCoastalTownJson(input); }
            var result = Generate(recipeJson, inputJson, dependencies, villageRecipeJson);
            if (!string.IsNullOrEmpty(result.roads_json))
            {
                var status = ApplyCoastalTownRoadsJson(island, result.roads_json);
                if (status == RoadGeometryRejected)
                {
                    Debug.LogWarning("Coastal settlements omitted after native road grading rejected the network: "
                        + Marshal.PtrToStringUTF8(GetLastCoastalTownError()));
                    return null;
                }
                if (status != 1) throw Error("Town road grading", GetLastCoastalTownError());
            }
            if (SetCoastalTownClearanceJson(island, result.clearance_json) != 1)
                throw Error("Town decoration clearance", GetLastCoastalTownError());
            if (connectSettlements && villageRecipeJson != null
                && ((result.road_exits?.Length ?? 0) > 0 || connectCoastalSeams))
                result.cartTracks = PrepareCartTracks(island, result, connectCoastalSeams);
            return result;
        }

        [Serializable]
        private sealed class CartTrackRequest
        {
            public TownRoadExit[] exits;
            public TownTrackBlocker[] blockers;
            public bool coast_seams;
            public int settlement_count;
        }

        private static TownCartTrackPlan PrepareCartTracks(IntPtr island, TownPreparedData town, bool coastSeams)
        {
            var request = new CartTrackRequest
            {
                exits = town.road_exits ?? Array.Empty<TownRoadExit>(),
                blockers = town.routing_blockers ?? Array.Empty<TownTrackBlocker>(),
                coast_seams = coastSeams,
                settlement_count = town.settlements?.Length ?? 0,
            };
            var report = PrepareCoastalCartTracks(island, JsonUtility.ToJson(request));
            if (report == IntPtr.Zero)
            {
                Debug.LogWarning("Cart tracks could not be prepared: " + Marshal.PtrToStringUTF8(GetLastCoastalTownError()));
                return null;
            }
            try
            {
                var plan = JsonUtility.FromJson<TownCartTrackPlan>(Marshal.PtrToStringUTF8(report));
                var diagnostics = new List<TownCartTrackDiagnostic>(
                    plan.diagnostics ?? Array.Empty<TownCartTrackDiagnostic>());
                for (var index = 0; index < request.settlement_count; index++)
                {
                    if (town.settlements[index].building_count == 0
                        || Array.Exists(request.exits, exit => exit.settlement_index == index)) continue;
                    diagnostics.Add(new TownCartTrackDiagnostic
                    {
                        settlement_index = index,
                        reason = "no_outward_road_exit",
                    });
                }
                plan.diagnostics = diagnostics.ToArray();
                return plan;
            }
            finally { FreeCoastalTownJson(report); }
        }

        internal static TownPreparedData Generate(string recipeJson, string inputJson,
            TownDependencyData[] dependencies = null, string villageRecipeJson = null)
        {
            if (GetMotuTownApiVersion() != ApiVersion)
                throw new InvalidOperationException("The native town building library does not match this package. Install the matching library and restart Unity.");
            using var files = new DependencyFiles(dependencies);
            var village = villageRecipeJson == null ? "" : ",\"village\":" + villageRecipeJson;
            var town = CreateMotuTown("{\"recipe\":" + recipeJson + ",\"input\":" + inputJson + village + "}",
                files.directory ?? "");
            if (town == IntPtr.Zero) throw Error("Town generation", GetLastMotuTownError());
            try
            {
                var json = ExportMotuTownJson(town);
                if (json == IntPtr.Zero) throw Error("Town export", GetLastMotuTownError());
                var result = JsonUtility.FromJson<TownPreparedData>(Marshal.PtrToStringUTF8(json));
                if (result == null || result.schema_version != ApiVersion || string.IsNullOrEmpty(result.clearance_json))
                    throw new InvalidOperationException("The native town returned an incompatible town package.");
                if (villageRecipeJson != null && result.settlements == null)
                    throw new InvalidOperationException("The native town building library does not support coastal settlements. Install the matching library and restart Unity.");
                if (result.road_count > 0 && (string.IsNullOrEmpty(result.roads_json) || (result.road_mesh?.indices?.Length ?? 0) == 0))
                    throw new InvalidOperationException("The native town road package is missing its terrain or mesh data.");
                return result;
            }
            finally { DestroyMotuTown(town); }
        }

        private static Exception Error(string operation, IntPtr message) =>
            new InvalidOperationException(operation + " failed: " + Marshal.PtrToStringUTF8(message));

        private sealed class DependencyFiles : IDisposable
        {
            internal readonly string directory;
            internal DependencyFiles(TownDependencyData[] dependencies)
            {
                if (dependencies == null || dependencies.Length == 0) return;
                directory = Path.Combine(Path.GetTempPath(), "MotuTownMaterials", Guid.NewGuid().ToString("N"));
                try
                {
                    Directory.CreateDirectory(directory);
                    foreach (var dependency in dependencies)
                        File.WriteAllText(Path.Combine(directory, dependency.fileName), dependency.json);
                }
                catch { Dispose(); throw; }
            }
            public void Dispose()
            {
                if (directory == null || !Directory.Exists(directory)) return;
                try { Directory.Delete(directory, true); }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
        }
    }
}
