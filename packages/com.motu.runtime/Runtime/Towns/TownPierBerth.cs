using UnityEngine;

namespace Motu.Towns
{
    /// <summary>Surveyed arrival and docking points for a capital pier.</summary>
    public sealed class TownPierBerth : MonoBehaviour
    {
        [SerializeField] private string pierId;
        [SerializeField] private Vector3 berth, approach, heading, landhead, deckEnd, walkway;
        [SerializeField] private float minimumDepth, length, width;
        [SerializeField] private bool accessConnected;

        public string PierId => pierId;
        /// <summary>Centre of the moored hull at mean sea level, in world space.</summary>
        public Vector3 Position => transform.TransformPoint(berth);
        /// <summary>Centre of the offshore turning area at mean sea level, in world space.</summary>
        public Vector3 ApproachPosition => transform.TransformPoint(approach);
        public Vector3 Forward => transform.TransformDirection(heading);
        public float Heading => Mathf.Atan2(Forward.x, Forward.z) * Mathf.Rad2Deg;
        public Vector3 LandheadPosition => transform.TransformPoint(landhead);
        public Vector3 DeckEndPosition => transform.TransformPoint(deckEnd);
        /// <summary>Top of the deck at the berth-side edge, aligned with the berth centre.</summary>
        public Vector3 WalkwayPosition => transform.TransformPoint(walkway);
        /// <summary>Minimum surveyed depth in metres.</summary>
        public float MinimumDepth => minimumDepth;
        public float Length => length;
        public float Width => width;
        public bool AccessConnected => accessConnected;

        internal void Configure(TownCapitalPier pier, Vector3 origin)
        {
            pierId = pier.id;
            berth = TownRenderer.Vector(pier.berth.position) - origin;
            approach = TownRenderer.Vector(pier.berth.approach) - origin;
            heading = TownRenderer.HorizontalDirection(pier.berth.heading);
            landhead = TownRenderer.Vector(pier.start) - origin;
            deckEnd = TownRenderer.Vector(pier.end) - origin;
            var deckCentre = deckEnd + Vector3.Project(berth - deckEnd, heading);
            var berthSide = Vector3.ProjectOnPlane(berth - deckCentre, Vector3.up).normalized;
            walkway = deckCentre + berthSide * (pier.width * .5f);
            minimumDepth = pier.berth.min_depth;
            length = pier.berth.length;
            width = pier.berth.width;
            accessConnected = pier.access_connected;
        }
    }
}
