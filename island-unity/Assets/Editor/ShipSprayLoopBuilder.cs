using System;
using System.Collections.Generic;
using Motu.Rendering;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace Motu.Editor
{
    internal static class ShipSprayLoopBuilder
    {
        internal static Vector2[] Build(ShipWaterlineTextureBuilder.Result hull, float spacing, int maximumSamples)
        {
            var width = hull.Texture.width;
            var height = hull.Texture.height;
            var edges = new Dictionary<Vector2Int, List<Vector2Int>>();
            bool Filled(int x, int y) => x >= 0 && y >= 0 && x < width && y < height && hull.HullPixels[y * width + x];
            void Edge(int x, int y, int dx, int dy)
            {
                var a = new Vector2Int(x, y);
                if (!edges.TryGetValue(a, out var destinations)) edges[a] = destinations = new List<Vector2Int>(2);
                destinations.Add(a + new Vector2Int(dx, dy));
            }
            // Directed cell boundaries keep the filled hull on the left. Unlike the
            // padded texture bounds, this outline is the actual filled waterline.
            for (var y = 0; y < height; y++)
            for (var x = 0; x < width; x++)
            {
                if (!Filled(x, y)) continue;
                if (!Filled(x, y - 1)) Edge(x, y, 1, 0);
                if (!Filled(x + 1, y)) Edge(x + 1, y, 0, 1);
                if (!Filled(x, y + 1)) Edge(x + 1, y + 1, -1, 0);
                if (!Filled(x - 1, y)) Edge(x, y + 1, 0, -1);
            }
            List<Vector2> outer = null;
            var largestArea = 0f;
            while (edges.Count > 0)
            {
                using var keys = edges.Keys.GetEnumerator();
                keys.MoveNext();
                var start = keys.Current;
                var current = start;
                var previousDirection = Vector2Int.zero;
                var loop = new List<Vector2>();
                do
                {
                    loop.Add(hull.TextureBounds.min + Vector2.Scale(current, hull.TextureBounds.size / new Vector2(width, height)));
                    if (!edges.TryGetValue(current, out var destinations))
                        throw new InvalidOperationException("The spray outline is open. Regenerate the waterline silhouette.");
                    var choice = 0;
                    // At diagonally touching cells keep each boundary turning left,
                    // rather than making a crossing through the shared corner.
                    if (destinations.Count > 1)
                    {
                        var bestTurn = float.NegativeInfinity;
                        for (var i = 0; i < destinations.Count; i++)
                        {
                            var direction = destinations[i] - current;
                            var turn = previousDirection.x * direction.y - previousDirection.y * direction.x;
                            if (turn > bestTurn) { bestTurn = turn; choice = i; }
                        }
                    }
                    var next = destinations[choice];
                    destinations.RemoveAt(choice);
                    if (destinations.Count == 0) edges.Remove(current);
                    previousDirection = next - current;
                    current = next;
                } while (current != start);
                var area = SignedArea(loop);
                if (area > largestArea) { largestArea = area; outer = loop; }
            }
            if (outer == null) throw new InvalidOperationException("No closed outer hull contour was found.");
            return Resample(outer, spacing, maximumSamples);
        }

        internal static float SignedArea(IReadOnlyList<Vector2> loop)
        {
            var area = 0f;
            for (var i = 0; i < loop.Count; i++)
            {
                var a = loop[i];
                var b = loop[(i + 1) % loop.Count];
                area += a.x * b.y - b.x * a.y;
            }
            return area * .5f;
        }

        internal static Vector2[] Resample(IReadOnlyList<Vector2> loop, float spacing, int maximumSamples)
        {
            var perimeter = 0f;
            for (var i = 0; i < loop.Count; i++) perimeter += Vector2.Distance(loop[i], loop[(i + 1) % loop.Count]);
            if (perimeter <= .001f) throw new InvalidOperationException("The hull waterline has no usable perimeter.");
            var count = Mathf.Clamp(Mathf.CeilToInt(perimeter / Mathf.Max(.1f, spacing)), 3, Mathf.Clamp(maximumSamples, 3, 512));
            var points = new Vector2[count];
            var edge = 0;
            var consumed = 0f;
            for (var i = 0; i < count; i++)
            {
                var distance = i * perimeter / count;
                var length = Vector2.Distance(loop[edge], loop[(edge + 1) % loop.Count]);
                while (edge + 1 < loop.Count && consumed + length < distance)
                {
                    consumed += length;
                    edge++;
                    length = Vector2.Distance(loop[edge], loop[(edge + 1) % loop.Count]);
                }
                points[i] = Vector2.Lerp(loop[edge], loop[(edge + 1) % loop.Count], length > 0 ? (distance - consumed) / length : 0);
            }
            return points;
        }

        internal static OceanHullSpray Assign(Transform root, ShipWaterlineTextureBuilder.Result hull, Vector3 forward, float worldY)
        {
            var spray = root.GetComponent<OceanHullSpray>();
            var outline = Build(hull, spray != null ? spray.SampleSpacing : 1f, spray != null ? spray.MaximumSamples : 256);
            var points = new Vector3[outline.Length];
            var origin = root.position;
            origin.y = worldY;
            var right = Vector3.Cross(Vector3.up, forward);
            for (var i = 0; i < outline.Length; i++)
                points[i] = root.InverseTransformPoint(origin + right * outline[i].x + forward * outline[i].y);
            const string materialPath = "Assets/Materials/ShipWaterlineSpray.mat";
            var material = AssetDatabase.LoadAssetAtPath<Material>(materialPath);
            if (material == null)
            {
                var shader = Shader.Find("Motu/Waterfall Spray Particle");
                if (shader == null) throw new InvalidOperationException("The spray particle shader is unavailable.");
                material = new Material(shader) { name = "Ship Waterline Spray" };
                AssetDatabase.CreateAsset(material, materialPath);
            }
            if (spray == null) spray = Undo.AddComponent<OceanHullSpray>(root.gameObject);
            Undo.RecordObject(spray, "Assign ship spray waterline");
            spray.SetWaterline(points, material);
            EditorUtility.SetDirty(spray);
            PrefabUtility.RecordPrefabInstancePropertyModifications(spray);
            EditorSceneManager.MarkSceneDirty(root.gameObject.scene);
            return spray;
        }

        [MenuItem("Island/Setup Selected Ship Waterline Spray")]
        private static void SetupSelected()
        {
            if (EditorApplication.isPlaying) { Debug.LogWarning("Exit Play Mode before authoring the spray waterline."); return; }
            var selected = Selection.activeTransform;
            if (selected == null) { Debug.LogWarning("Select the ship first."); return; }
            var body = selected.GetComponentInParent<Rigidbody>();
            var root = body != null ? body.transform : selected;
            var forward = Vector3.ProjectOnPlane(root.forward, Vector3.up).normalized;
            var clamp = root.GetComponent<OceanDeckWaveClamp>();
            if (clamp != null) clamp.GetFootprint(out _, out forward);
            var worldY = ShipWaterlineTextureWindow.EstimateWaterline(root);
            ShipWaterlineTextureBuilder.Result hull = null;
            try
            {
                hull = ShipWaterlineTextureBuilder.Build(ShipWaterlineTextureBuilder.Slice(root, null, worldY, forward),
                    new ShipWaterlineTextureBuilder.Settings());
                var spray = Assign(root, hull, forward, worldY);
                Selection.activeGameObject = root.gameObject;
                Debug.Log($"Assigned {spray.SampleCount} connected waterline spray samples to {root.name}. Save the scene to keep the assignment.", root);
            }
            finally { if (hull != null) UnityEngine.Object.DestroyImmediate(hull.Texture); }
        }
    }
}
