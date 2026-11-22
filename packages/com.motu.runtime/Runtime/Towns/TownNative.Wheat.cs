using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Motu.Towns
{
    internal static partial class TownNative
    {
        [DllImport("motu", CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr PrepareCoastalWheatFields(IntPtr island,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string json);

        [Serializable] private sealed class WheatFields
        {
            public TownTrackBlocker[] fields;
            public TownTrackBlocker[] bridge_clearance;
        }

        private static TownTrackBlocker[] PrepareWheatFields(IntPtr island, TownPreparedData town)
        {
            var request = new WheatFields {
                fields = town.wheat_fields, bridge_clearance = WaterfrontClearance(town, 2f).ToArray()
            };
            var report = PrepareCoastalWheatFields(island, JsonUtility.ToJson(request));
            if (report == IntPtr.Zero) throw Error("Wheat field preparation", GetLastCoastalTownError());
            try
            {
                return JsonUtility.FromJson<WheatFields>(Marshal.PtrToStringUTF8(report))?.fields
                    ?? throw new InvalidOperationException("The native wheat field report is invalid.");
            }
            finally { FreeCoastalTownJson(report); }
        }

        private static List<TownTrackBlocker> WaterfrontClearance(TownPreparedData town, float landingMargin)
        {
            var bridgeClearance = new List<TownTrackBlocker>();
            foreach (var bridge in town.bridgePlan?.bridges ?? Array.Empty<TownBridge>())
            {
                var a = new Vector2(bridge.start[0], bridge.start[1]);
                var b = new Vector2(bridge.end[0], bridge.end[1]);
                var mid = (a + b) * 0.5f;
                var span = b - a;
                bridgeClearance.Add(new TownTrackBlocker {
                    centre = new[] { mid.x, mid.y },
                    half_extents = new[] { span.magnitude * 0.5f + landingMargin, bridge.width * 0.5f + 2f },
                    yaw = Mathf.Atan2(span.y, span.x)
                });
            }
            if (town.pier?.clearance != null) bridgeClearance.Add(town.pier.clearance);

            return bridgeClearance;
        }
    }
}
