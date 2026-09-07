using System;
using UnityEngine;
using Motu.Gameplay;
using Motu.World;

namespace Motu.Editor
{
    public static class MinimapTeleportValidation
    {

        private sealed class Surface : IWorldSurfaceQuery
        {
            public float Height;
            public Transform Target;
            public Vector3 Prepared;
            public void SetStreamingTarget(Transform target) => Target = target;
            public void PrepareStreamingAt(Vector3 point) => Prepared = point;
            public bool TrySnapToTerrain(Vector3 point, out Vector3 ground)
            { ground = point; return false; }
            public float GetTerrainOrSeaHeight(Vector3 point) => Height;
            public void SetFirstPersonViewActive(bool active) { }
        }

        public static void BatchValidateMinimapTeleport()
        {
            var root = new GameObject("Minimap teleport validation");
            root.SetActive(false);
            var cursorLock = Cursor.lockState;
            var cursorVisible = Cursor.visible;
            var demo = root.AddComponent<IslandDemoController>();
            var camera = root.AddComponent<Camera>();
            var orbit = root.AddComponent<OrbitCamera>();
            var player = root.AddComponent<FirstPersonController>();
            var manager = root.AddComponent<IslandWorldManager>();
            var surface = new Surface();
            try
            {
                orbit.Configure(Vector3.zero, 200f);
                player.Configure(orbit, surface);
                demo.Configure(null, camera, orbit, player);
                demo.minimapTexture = new Texture2D(1, 1);
                demo.hasMinimapCentre = true;
                var centre = new Vector2Int(-10, 23);
                demo.minimapCentreCell = centre;
                var map = IslandDemoController.MinimapMapRect();
                Vector2 Point(int column, int row) => new Vector2(map.x + (column + .5f) * 7f,
                    map.y + (row + .5f) * 7f);
                foreach (var square in new[] { Vector2Int.zero, new Vector2Int(32, 0),
                    new Vector2Int(0, 32), new Vector2Int(32, 32), new Vector2Int(16, 16) })
                {
                    Require(demo.TryGetMinimapCell(Point(square.x, square.y), out var cell)
                        && cell == centre + new Vector2Int(square.x - 16, 16 - square.y),
                        "Minimap corners/centre must match the displayed world cell, including negative cells.");
                }
                foreach (var point in new[] { new Vector2(map.x - 1, map.y),
                    new Vector2(map.xMax, map.y), new Vector2(map.x, map.yMax) })
                    Require(!demo.TryGetMinimapCell(point, out _),
                        "Map margins and exclusive right/bottom edges must not teleport.");

                var start = root.transform.position;
                var rotation = root.transform.rotation;
                var click = Point(32, 0);
                demo.HandleMinimapPointer(click, true, false, true);
                Require(root.transform.position == start, "Mouse-down must not teleport before release.");
                demo.HandleMinimapPointer(click, false, true, true);
                var destination = IslandWorldManager.CellCentre(centre + new Vector2Int(16, 16));
                Require(root.transform.position == destination + Vector3.up * player.FlyClearanceMetres
                    && player.IsActive && player.IsFlyMode && player.IsCursorReleased
                    && !root.GetComponent<CharacterController>().enabled && !orbit.enabled
                    && surface.Target == root.transform && surface.Prepared.x == destination.x
                    && Quaternion.Angle(root.transform.rotation, rotation) < .001f,
                    "Clicking an unloaded square must move to its centre in fly mode, preserving heading and released cursor.");
                Require(orbit.Target == destination,
                    "Exiting to overview must retain the destination cell.");

                surface.Height = 80f;
                player.UpdateMovement();
                Require(Mathf.Abs(root.transform.position.y - 80f - player.FlyClearanceMetres) < .001f,
                    "Released-cursor flight must rise above terrain as it loads.");
                var beforeDrag = root.transform.position;
                demo.HandleMinimapPointer(Point(16, 16), true, false, true);
                demo.HandleMinimapPointer(Point(18, 16), false, true, true);
                Require(root.transform.position == beforeDrag, "Dragging across squares must not teleport.");
                demo.HandleMinimapPointer(Point(16, 16), true, false, true);
                demo.HandleMinimapPointer(Point(18, 16), false, false, true);
                demo.HandleMinimapPointer(Point(16, 16), false, true, true);
                Require(root.transform.position == beforeDrag, "Returning a drag to its starting square must not teleport.");
                demo.HandleMinimapPointer(Point(16, 16), true, false, false);
                demo.HandleMinimapPointer(Point(16, 16), false, true, false);
                Require(root.transform.position == beforeDrag, "Captured-cursor clicks must not teleport.");
                demo.ShowMinimap = false;
                demo.HandleMinimapPointer(Point(16, 16), true, false, true);
                demo.HandleMinimapPointer(Point(16, 16), false, true, true);
                Require(root.transform.position == beforeDrag, "Hidden maps must not receive clicks.");

                manager.hasPreviousTargetPosition = true;
                manager.smoothedVelocity = Vector3.one * 10000f;
                manager.nextDiscoveryTime = float.MaxValue;
                manager.SetStreamingTarget(root.transform);
                Require(!manager.hasPreviousTargetPosition
                    && manager.smoothedVelocity == Vector3.zero
                    && manager.nextDiscoveryTime == 0f
                    && manager.LogicalPlayerPosition == new Vector2(destination.x, destination.z),
                    "Teleport must reset travel prediction and refresh discovery at the destination.");
                player.Exit();
                Require(orbit.enabled && !player.IsActive, "Overview must still work after teleporting.");
                Debug.Log("Minimap teleport validation passed: cell orientation and edges, click/drag routing, "
                    + "hidden/captured input, safe fly arrival, late terrain loading, overview and streaming reset.");
            }
            finally
            {
                UnityEngine.Object.DestroyImmediate(root);
                Cursor.lockState = cursorLock;
                Cursor.visible = cursorVisible;
            }
        }

        private static void Require(bool success, string message)
        {
            if (!success) throw new InvalidOperationException(message);
        }
    }
}
