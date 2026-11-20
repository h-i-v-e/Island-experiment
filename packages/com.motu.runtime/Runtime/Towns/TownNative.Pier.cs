using System;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Towns
{
    internal static partial class TownNative
    {
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
            try
            {
                var plan = JsonUtility.FromJson<CapitalPierReport>(Marshal.PtrToStringUTF8(report));
                town.pier = plan?.pier?.berth != null ? plan.pier : null;
                town.pierDiagnostic = plan?.reason;
            }
            finally { FreeCoastalTownJson(report); }
        }
    }
}
