using System;
using System.Collections.Generic;
using UnityEngine;

namespace Motu.Streaming
{
    /// <summary>Single-player cave mode: suspend exterior collision without
    /// cutting heightfield holes, preserving roof collision when back outside.</summary>
    internal sealed class CaveCollisionScope : IDisposable
    {
        private static CaveCollisionScope current;
        private readonly HashSet<Collider> suspended = new HashSet<Collider>();
        private readonly CharacterController player;
        internal bool IsInside { get; private set; }

        internal CaveCollisionScope(CharacterController player) { this.player = player; }

        internal void Update(Vector3 feet, bool walking)
        {
            var inside = false;
            if (walking)
                foreach (var cave in UnityEngine.Object.FindObjectsByType<CaveStreamer>())
                    if (cave.TryFindGround(feet, .3f, 2f, out _)) { inside = true; break; }
            if (!inside) { Dispose(); return; }
            current = this;
            IsInside = true;
            // Also catches streamed or newly spawned exterior colliders before Move.
            foreach (var collider in UnityEngine.Object.FindObjectsByType<Collider>())
                Suspend(collider);
        }

        internal static void RegisterExteriorCollider(Collider collider) => current?.Suspend(collider);

        private void Suspend(Collider collider)
        {
            if (collider == null || !collider.enabled || collider == player
                || collider.transform.IsChildOf(player.transform)
                || (collider.GetComponentInParent<CaveStreamer>() is CaveStreamer cave && cave.OwnsCollider(collider))) return;
            suspended.Add(collider);
            collider.enabled = false;
        }

        public void Dispose()
        {
            foreach (var collider in suspended)
                if (collider != null) collider.enabled = true;
            suspended.Clear();
            IsInside = false;
            if (current == this) current = null;
        }
    }
}
