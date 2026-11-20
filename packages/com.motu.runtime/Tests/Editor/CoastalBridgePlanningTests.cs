using System;
using System.Collections.Generic;
using Motu.Towns;
using NUnit.Framework;

namespace Motu.Tests
{
    public sealed class CoastalBridgePlanningTests
    {
        private static TownBridgeCandidate Candidate(float x) => new TownBridgeCandidate {
            from = new[] { x, 55f, 12f }, to = new[] { x + 24f, 57f, 13f }, width = 4f,
        };

        [Test]
        public void ReplanningExcludesOnlyRejectedLandingsAndKeepsFinalFittedGeometry()
        {
            var bad = Candidate(1.25f);
            var good = Candidate(140f);
            var bridge = new TownBridge { id = "retained crossing", start = good.to, end = good.from };
            var json = new List<string>();
            TownPreparedData Generate(string input)
            {
                json.Add(input);
                return new TownPreparedData { bridge_candidates = json.Count == 1 ? new[] { good, bad } : new[] { good } };
            }
            var town = TownNative.PlanPavedBridges("{\"terrain\":{}}", Generate,
                _ => new TownBridgePlan { bridges = new[] { bridge } });
            Assert.That(json, Has.Count.EqualTo(2));
            Assert.That(json[1], Is.EqualTo("{\"terrain\":{},\"bridge_exclusions\":[[[1.25,55],[25.25,57]]]}"));
            Assert.That(town.bridge_candidates, Is.EqualTo(new[] { good }));
            Assert.That(town.bridgePlan.bridges[0], Is.SameAs(bridge));
            var diagnostic = town.bridgePlan.diagnostics[0];
            Assert.That(diagnostic.crossing, Is.EqualTo(1));
            Assert.That(diagnostic.start, Is.SameAs(bad.from));
            Assert.That(diagnostic.surface, Is.EqualTo("paved"));
        }

        [Test]
        public void BoundedRetryReplansDryRoadsAndRetainsSpecificRejectionReasons()
        {
            var inputs = new List<string>();
            TownPreparedData Generate(string input)
            {
                inputs.Add(input);
                return new TownPreparedData {
                    bridge_candidates = input.Contains("\"disable_bridges\":true")
                        ? Array.Empty<TownBridgeCandidate>() : new[] { Candidate(45f * inputs.Count) },
                };
            }
            var town = TownNative.PlanPavedBridges("{\"tile_seed\":1}", Generate,
                _ => new TownBridgePlan { diagnostics = new[] { new TownBridgeDiagnostic { crossing = 0, reason = "ridge" } } });
            Assert.That(inputs.Count, Is.EqualTo(4));
            Assert.That(inputs[2], Is.EqualTo("{\"tile_seed\":1,\"bridge_exclusions\":[[[45,55],[69,57]],[[90,55],[114,57]]]}"));
            Assert.That(town.bridge_candidates, Is.Empty);
            Assert.That(town.bridgePlan.diagnostics, Has.Length.EqualTo(3));
            Assert.That(town.bridgePlan.diagnostics[2].reason, Is.EqualTo("ridge"));
            Assert.That(town.bridgePlan.diagnostics[2].kind, Is.EqualTo("river"));
        }
    }
}
