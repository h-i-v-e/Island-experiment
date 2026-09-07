using System;
using System.Reflection;
using UnityEngine;
using Motu.Gameplay;
using Motu.World;

namespace Motu.Editor
{
    public static class MinimapTeleportValidation
    {
        private const BindingFlags Private = BindingFlags.NonPublic | BindingFlags.Instance;

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
                Set(demo, "minimapTexture", new Texture2D(1, 1));
                Set(demo, "hasMinimapCentre", true);
                var centre = new Vector2Int(-10, 23);
                Set(demo, "minimapCentreCell", centre);
                var map = (Rect)typeof(IslandDemoController).GetMethod("MinimapMapRect",
                    BindingFlags.NonPublic | BindingFlags.Static).Invoke(null, null);
                Vector2 Point(int column, int row) => new Vector2(map.x + (column + .5f) * 7f,
                    map.y + (row + .5f) * 7f);
                foreach (var square in new[] { Vector2Int.zero, new Vector2Int(32, 0),
                    new Vector2Int(0, 32), new Vector2Int(32, 32), new Vector2Int(16, 16) })
                {
                    var arguments = new object[] { Point(square.x, square.y), null };
                    Require((bool)Invoke(demo, "TryGetMinimapCell", arguments)
                        && (Vector2Int)arguments[1] == centre + new Vector2Int(square.x - 16, 16 - square.y),
                        "Minimap corners/centre must match the displayed world cell, including negative cells.");
                }
                foreach (var point in new[] { new Vector2(map.x - 1, map.y),
                    new Vector2(map.xMax, map.y), new Vector2(map.x, map.yMax) })
                    Require(!(bool)Invoke(demo, "TryGetMinimapCell", point, null),
                        "Map margins and exclusive right/bottom edges must not teleport.");

                var start = root.transform.position;
                var rotation = root.transform.rotation;
                var click = Point(32, 0);
                Invoke(demo, "HandleMinimapPointer", click, true, false, true);
                Require(root.transform.position == start, "Mouse-down must not teleport before release.");
                Invoke(demo, "HandleMinimapPointer", click, false, true, true);
                var destination = IslandWorldManager.CellCentre(centre + new Vector2Int(16, 16));
                Require(root.transform.position == destination + Vector3.up * player.FlyClearanceMetres
                    && player.IsActive && player.IsFlyMode && player.IsCursorReleased
                    && !root.GetComponent<CharacterController>().enabled && !orbit.enabled
                    && surface.Target == root.transform && surface.Prepared.x == destination.x
                    && Quaternion.Angle(root.transform.rotation, rotation) < .001f,
                    "Clicking an unloaded square must move to its centre in fly mode, preserving heading and released cursor.");
                Require((Vector3)Get(orbit, "target") == destination,
                    "Exiting to overview must retain the destination cell.");

                surface.Height = 80f;
                Invoke(player, "Update");
                Require(Mathf.Abs(root.transform.position.y - 80f - player.FlyClearanceMetres) < .001f,
                    "Released-cursor flight must rise above terrain as it loads.");
                var beforeDrag = root.transform.position;
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), true, false, true);
                Invoke(demo, "HandleMinimapPointer", Point(18, 16), false, true, true);
                Require(root.transform.position == beforeDrag, "Dragging across squares must not teleport.");
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), true, false, true);
                Invoke(demo, "HandleMinimapPointer", Point(18, 16), false, false, true);
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), false, true, true);
                Require(root.transform.position == beforeDrag, "Returning a drag to its starting square must not teleport.");
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), true, false, false);
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), false, true, false);
                Require(root.transform.position == beforeDrag, "Captured-cursor clicks must not teleport.");
                demo.ShowMinimap = false;
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), true, false, true);
                Invoke(demo, "HandleMinimapPointer", Point(16, 16), false, true, true);
                Require(root.transform.position == beforeDrag, "Hidden maps must not receive clicks.");

                Set(manager, "hasPreviousTargetPosition", true);
                Set(manager, "smoothedVelocity", Vector3.one * 10000f);
                Set(manager, "nextDiscoveryTime", float.MaxValue);
                manager.SetStreamingTarget(root.transform);
                Require(!(bool)Get(manager, "hasPreviousTargetPosition")
                    && (Vector3)Get(manager, "smoothedVelocity") == Vector3.zero
                    && (float)Get(manager, "nextDiscoveryTime") == 0f
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

        private static object Invoke(object target, string method, params object[] args)
            => target.GetType().GetMethod(method, Private).Invoke(target, args);
        private static object Get(object target, string name)
            => target.GetType().GetField(name, Private).GetValue(target);
        private static void Set(object target, string name, object value)
            => target.GetType().GetField(name, Private).SetValue(target, value);
        private static void Require(bool success, string message)
        {
            if (!success) throw new InvalidOperationException(message);
        }
    }
}
