using System;
using System.Globalization;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Towns
{
    internal static partial class TownNative
    {
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr PrepareCoastalRoadsideFields(IntPtr island,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string json);

        [Serializable] private sealed class RoadsideFieldRequest
        {
            public string seed;
            public float[] wind_direction;
            public TownTrackBlocker[] blockers;
        }

        private static TownRoadsideFieldPlan PrepareRoadsideFields(IntPtr island, TownPreparedData town,
            ulong seed, Vector2? prevailingWind)
        {
            var wind = prevailingWind ?? new Vector2(1f, 0.25f);
            if (!float.IsFinite(wind.x) || !float.IsFinite(wind.y)) wind = new Vector2(1f, 0.25f);
            var exclusion = WaterfrontClearance(town, 12f);
            exclusion.AddRange(town.routing_blockers ?? Array.Empty<TownTrackBlocker>());
            exclusion.AddRange(town.wheat_fields ?? Array.Empty<TownTrackBlocker>());
            var request = new RoadsideFieldRequest {
                seed = seed.ToString(CultureInfo.InvariantCulture), wind_direction = new[] { wind.x, wind.y },
                blockers = exclusion.ToArray(),
            };
            var report = PrepareCoastalRoadsideFields(island, JsonUtility.ToJson(request));
            if (report == IntPtr.Zero) throw Error("Roadside field preparation", GetLastCoastalTownError());
            try
            {
                return JsonUtility.FromJson<TownRoadsideFieldPlan>(Marshal.PtrToStringUTF8(report))
                    ?? throw new InvalidOperationException("The native roadside field report is invalid.");
            }
            finally { FreeCoastalTownJson(report); }
        }
    }
}
