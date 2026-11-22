using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Towns
{
    internal static partial class TownNative
    {
        [DllImport("motu_building", CallingConvention = CallingConvention.Cdecl)]
        private static extern int FindMotuCapitalMouth([MarshalAs(UnmanagedType.LPUTF8Str)] string input,
            byte multiple, [Out] float[] position);

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr SurveyCoastalCapitalPiers(IntPtr island, float mouthX, float mouthY);

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr ExportPavedCapitalPier(IntPtr island,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string layout, ulong seed);

        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr PrepareCoastalCapitalPier(IntPtr island,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string json);

        [Serializable] private sealed class CapitalPierRequest
        {
            public float[] centre, mouth;
            public TownTrackBlocker[] blockers;
            public ulong seed;
        }

        [Serializable] private sealed class CapitalPierReport
        {
            public TownCapitalPier pier;
            public string reason;
        }

        private static string SurveyPierCandidates(IntPtr island, string input, bool multiple)
        {
            var position = new float[2];
            var status = FindMotuCapitalMouth(input, (byte)(multiple ? 1 : 0), position);
            if (status < 0) throw Error("Capital outlet survey", GetLastMotuTownError());
            if (status == 0) return null;
            var survey = SurveyCoastalCapitalPiers(island, position[0], position[1]);
            if (survey == IntPtr.Zero) throw Error("Capital pier survey", GetLastCoastalTownError());
            try { return Marshal.PtrToStringUTF8(survey); }
            finally { FreeCoastalTownJson(survey); }
        }

        private static void ExportCapitalPier(IntPtr island, TownPreparedData town, ulong seed)
        {
            if (!string.IsNullOrEmpty(town.pier_layout_json))
            {
                var report = ExportPavedCapitalPier(island, town.pier_layout_json, seed);
                if (report == IntPtr.Zero) throw Error("Paved capital pier export", GetLastCoastalTownError());
                ReadCapitalPier(town, report);
            }
            if (town.pier == null)
            {
                town.pier_layout_json = null;
                PrepareCapitalPier(island, town, seed);
            }
        }

        private static void PrepareCapitalPier(IntPtr island, TownPreparedData town, ulong seed)
        {
            var capital = Array.Find(town.settlements ?? Array.Empty<TownSettlement>(),
                settlement => settlement.kind == "capital" && settlement.building_count > 0);
            if (capital?.centre?.Length != 2 || capital.mouth?.Length != 2) return;

            var request = new CapitalPierRequest
            {
                centre = capital.centre,
                mouth = capital.mouth,
                blockers = town.routing_blockers ?? Array.Empty<TownTrackBlocker>(),
                seed = seed,
            };
            IntPtr report;
            try { report = PrepareCoastalCapitalPier(island, JsonUtility.ToJson(request)); }
            catch (EntryPointNotFoundException)
            {
                town.pierDiagnostic = "native_pier_unavailable";
                Debug.LogWarning("The loaded island library does not support capital piers. Install the matching library and restart Unity.");
                return;
            }
            if (report == IntPtr.Zero)
            {
                town.pierDiagnostic = "native_pier_preparation_failed";
                Debug.LogWarning("Capital pier preparation failed: " + Marshal.PtrToStringUTF8(GetLastCoastalTownError()));
                return;
            }
            ReadCapitalPier(town, report);
        }

        private static void ReadCapitalPier(TownPreparedData town, IntPtr report)
        {
            try { ApplyCapitalPierReport(town, Marshal.PtrToStringUTF8(report)); }
            finally { FreeCoastalTownJson(report); }
        }

        internal static void ApplyCapitalPierReport(TownPreparedData town, string json)
        {
            var plan = JsonUtility.FromJson<CapitalPierReport>(json);
            // JsonUtility can materialize a null pier and its nested clearance as empty objects.
            var pier = plan?.pier;
            town.pier = !string.IsNullOrEmpty(pier?.id) && pier.berth != null ? pier : null;
            town.pierDiagnostic = plan?.reason;
        }
    }
}
