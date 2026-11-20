using System;
using System.Collections.Generic;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;

namespace Motu.Towns
{
    internal static partial class TownNative
    {
        private const int MaximumPavedFitAttempts = 3;
        private const float SameLandingMetres = .35f;

        [Serializable] private sealed class BridgeRequest
        {
            public List<BridgeCrossing> crossings;
            public string roads_json;
            public int existing_bridge_count, existing_vertex_count;
        }

        [Serializable] private sealed class BridgeCrossing
        {
            [NonSerialized] public int sourceIndex;
            public float[] start, end, start_heading, end_heading;
            public float width;
            public string kind, surface;
        }

        internal static TownPreparedData PlanPavedBridges(string inputJson, Func<string, TownPreparedData> generate,
            Func<TownPreparedData, TownBridgePlan> fit)
        {
            var rejected = new List<TownBridgeDiagnostic>();
            var town = generate(inputJson);
            for (var attempt = 1; (town.bridge_candidates?.Length ?? 0) != 0; attempt++)
            {
                var plan = fit(town);
                var misses = RejectedPavedCrossings(town.bridge_candidates, plan);
                if (misses.Count == 0)
                {
                    town.bridgePlan = plan;
                    break;
                }
                var repeated = false;
                foreach (var miss in misses)
                {
                    if (rejected.Exists(old => SameCrossing(old.start, old.end, miss.start, miss.end)))
                        repeated = true;
                    else rejected.Add(miss);
                }
                var disable = repeated || attempt >= MaximumPavedFitAttempts;
                town = generate(PlanningInput(inputJson, rejected, disable));
                if (disable && (town.bridge_candidates?.Length ?? 0) > 0)
                    throw new InvalidOperationException("The town planner ignored disabled bridge crossings. Install the matching building library and restart Unity.");
            }
            if (rejected.Count != 0)
            {
                town.bridgePlan ??= new TownBridgePlan();
                town.bridgePlan.diagnostics = rejected.ToArray();
            }
            return town;
        }

        private static List<TownBridgeDiagnostic> RejectedPavedCrossings(TownBridgeCandidate[] candidates, TownBridgePlan plan)
        {
            var misses = new List<TownBridgeDiagnostic>();
            for (var index = 0; index < candidates.Length; index++)
            {
                var road = candidates[index];
                var diagnostic = Array.Find(plan.diagnostics ?? Array.Empty<TownBridgeDiagnostic>(), d => d.crossing == index);
                if (diagnostic == null && ContainsBridge(plan, road.from, road.to)) continue;
                diagnostic ??= new TownBridgeDiagnostic { crossing = index, reason = "missing_fitted_paved_crossing" };
                diagnostic.start = road.from;
                diagnostic.end = road.to;
                diagnostic.surface = "paved";
                diagnostic.kind = "river";
                misses.Add(diagnostic);
            }
            return misses;
        }

        // The island exports a complete JSON object. Append just the optional planner inputs,
        // avoiding a second managed parse of the million-sample height grid.
        private static string PlanningInput(string original, List<TownBridgeDiagnostic> rejected, bool disable)
        {
            var json = new StringBuilder(original.Length + rejected.Count * 64);
            json.Append(original, 0, original.LastIndexOf('}'));
            if (disable) return json.Append(",\"disable_bridges\":true}").ToString();
            json.Append(",\"bridge_exclusions\":[");
            void Point(float[] point) => json.Append('[').Append(point[0].ToString("R", CultureInfo.InvariantCulture))
                .Append(',').Append(point[1].ToString("R", CultureInfo.InvariantCulture)).Append(']');
            for (var i = 0; i < rejected.Count; i++)
            {
                if (i != 0) json.Append(',');
                json.Append('[');
                Point(rejected[i].start);
                json.Append(',');
                Point(rejected[i].end);
                json.Append(']');
            }
            return json.Append("]}").ToString();
        }

        private static List<BridgeCrossing> PavedCrossings(TownPreparedData town)
        {
            var candidates = town.bridge_candidates ?? Array.Empty<TownBridgeCandidate>();
            var crossings = new List<BridgeCrossing>(candidates.Length);
            foreach (var road in candidates)
                crossings.Add(new BridgeCrossing {
                    sourceIndex = crossings.Count, start = road.from, end = road.to, width = road.width,
                    start_heading = road.start_heading, end_heading = road.end_heading,
                    kind = "river", surface = "paved",
                });
            return crossings;
        }

        private static void AddCartCrossings(List<BridgeCrossing> crossings, TownPreparedData town, TownBridgePlan paved)
        {
            var tracks = town.cartTracks?.bridges ?? Array.Empty<TownCartTrackBridge>();
            var pavedCount = town.bridge_candidates?.Length ?? 0;
            for (var i = 0; i < tracks.Length; i++)
            {
                var track = tracks[i];
                if (ContainsBridge(paved, track.start, track.end)) continue;
                crossings.Add(new BridgeCrossing {
                    sourceIndex = pavedCount + i, start = track.start, end = track.end, width = track.width,
                    start_heading = track.start_heading, end_heading = track.end_heading,
                    kind = track.kind, surface = "cart_track",
                });
            }
        }

        private static bool ContainsBridge(TownBridgePlan plan, float[] start, float[] end) =>
            plan?.bridges != null && Array.Exists(plan.bridges, b => SameCrossing(b.start, b.end, start, end));

        private static bool SameCrossing(float[] a, float[] b, float[] c, float[] d)
        {
            bool Near(float[] p, float[] q) =>
                (TownRenderer.Vector(p) - TownRenderer.Vector(q)).sqrMagnitude <= SameLandingMetres * SameLandingMetres;
            return (Near(a, c) && Near(b, d)) || (Near(a, d) && Near(b, c));
        }

        private static void AppendCartBridges(IntPtr island, TownPreparedData town)
        {
            if ((town.cartTracks?.bridges?.Length ?? 0) == 0) return;
            var paved = town.bridgePlan;
            var crossings = new List<BridgeCrossing>();
            AddCartCrossings(crossings, town, paved);
            if (crossings.Count == 0) return;
            var cart = FitBridges(island, crossings, existing: paved);
            if (paved == null) { town.bridgePlan = cart; return; }
            T[] Join<T>(T[] first, T[] second)
            {
                if (first == null || first.Length == 0) return second;
                if (second == null || second.Length == 0) return first;
                var joined = new T[first.Length + second.Length];
                Array.Copy(first, joined, first.Length);
                Array.Copy(second, 0, joined, first.Length, second.Length);
                return joined;
            }
            paved.bridges = Join(paved.bridges, cart.bridges);
            paved.diagnostics = Join(paved.diagnostics, cart.diagnostics);
        }

        private static TownBridgePlan FitBridges(IntPtr island, List<BridgeCrossing> crossings,
            string roads = null, TownBridgePlan existing = null)
        {
            var request = new BridgeRequest { crossings = crossings, roads_json = roads };
            foreach (var bridge in existing?.bridges ?? Array.Empty<TownBridge>())
            {
                request.existing_bridge_count++;
                request.existing_vertex_count += (bridge.masonry.positions.Length + bridge.dressing.positions.Length
                    + bridge.deck.positions.Length + bridge.coarse_masonry.positions.Length
                    + bridge.coarse_deck.positions.Length + bridge.collision.positions.Length) / 3;
            }
            IntPtr report;
            try { report = PrepareCoastalTownBridges(island, JsonUtility.ToJson(request)); }
            catch (EntryPointNotFoundException error)
            {
                throw new InvalidOperationException("The loaded island library does not support town bridges. Install the matching library and restart Unity.", error);
            }
            if (report == IntPtr.Zero) throw Error("Town bridge preparation", GetLastCoastalTownError());
            try
            {
                var plan = JsonUtility.FromJson<TownBridgePlan>(Marshal.PtrToStringUTF8(report));
                foreach (var diagnostic in plan.diagnostics ?? Array.Empty<TownBridgeDiagnostic>())
                {
                    var crossing = crossings[diagnostic.crossing];
                    diagnostic.crossing = crossing.sourceIndex;
                    diagnostic.kind = crossing.kind;
                    diagnostic.surface = crossing.surface;
                    diagnostic.start = crossing.start;
                    diagnostic.end = crossing.end;
                }
                return plan;
            }
            finally { FreeCoastalTownJson(report); }
        }
    }
}
