using System;
using System.IO;
using System.Text;
using Motu.Editor;
using Motu.Rendering;
using UnityEditor;
using UnityEngine;

public static class RowingBoatWaterExclusionSetup
{
    public const string VolumePath = BoatTestSceneSetup.Folder + "/RowingBoatWaterExclusion.asset";
    public const float DeckHeight = .85f;
    public const float HullInset = .025f;
    private const string ReportPath = "/tmp/rowing-boat-water-exclusion.txt";

    public static void Fit(OceanHullVolume volume, Mesh mesh, Matrix4x4 sourceToHull)
    {
        OceanHullVolumeBuilder.BakeMainHull(volume, mesh, sourceToHull, DeckHeight, HullInset);
    }

    [MenuItem("Island/Boat Tests/Bake Rowing Boat Water Exclusion")]
    public static void Bake()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode before baking.");
        var root = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
        try
        {
            var clamp = root.GetComponent<OceanDeckWaveClamp>();
            var walk = root.GetComponentInChildren<MeshFilter>();
            if (clamp == null || walk == null || walk.sharedMesh == null)
                throw new InvalidOperationException("Rowboat needs its detailed hull mesh and root water clamp.");
            var volume = AssetDatabase.LoadAssetAtPath<OceanHullVolume>(VolumePath);
            if (volume == null || clamp.ConvexHull != volume)
                throw new InvalidOperationException("Rowboat must reference its existing water-exclusion asset.");
            var previous = volume.Boundary.bounds;
            Fit(volume, walk.sharedMesh, clamp.transform.worldToLocalMatrix * walk.transform.localToWorldMatrix);
            // Keep the same asset GUID: existing scene/prefab references update without a scene save.
            AssetDatabase.SaveAssetIfDirty(volume);
            File.WriteAllText(ReportPath, $"Previous envelope: {previous}\nMain-shell interior: {volume.Boundary.bounds}\n");
        }
        finally { PrefabUtility.UnloadPrefabContents(root); }
        Validate();
    }

    [MenuItem("Island/Boat Tests/Validate Rowing Boat Water Exclusion")]
    public static void Validate()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Validate outside Play mode.");
        var report = new StringBuilder();
        var root = PrefabUtility.LoadPrefabContents(BoatTestSceneSetup.BoatPrefab);
        try
        {
            var volume = AssetDatabase.LoadAssetAtPath<OceanHullVolume>(VolumePath);
            var clamp = root.GetComponent<OceanDeckWaveClamp>();
            var source = root.GetComponentInChildren<MeshFilter>();
            Require(volume != null && volume.IsValid && clamp.ConvexHull == volume, "prefab retains the existing valid water-exclusion asset", report);
            var matrix = clamp.transform.worldToLocalMatrix * source.transform.localToWorldMatrix;
            var points = OceanHullVolumeBuilder.MainShellPoints(source.sharedMesh, matrix, DeckHeight);
            var shell = new Bounds(points[0], Vector3.zero);
            foreach (var point in points) shell.Encapsulate(point);
            var bounds = volume.Boundary.bounds;
            Require(bounds.max.y <= DeckHeight - HullInset + .002f, "upper edge capped below raised trim", report);
            Require(bounds.min.x > shell.min.x && bounds.max.x < shell.max.x
                && bounds.min.y > shell.min.y && bounds.min.z > shell.min.z && bounds.max.z < shell.max.z,
                "boundary inset inside the main shell on every axis", report);
            Require(bounds.min.y > .05f && bounds.min.z > -2.4f && bounds.max.z < 2.6f,
                "keel and end projections no longer enlarge the water exclusion", report);
            bool Contains(Vector3 point)
            {
                foreach (var plane in volume.Planes)
                    if (Vector3.Dot(new Vector3(plane.x, plane.y, plane.z), point) + plane.w > .001f) return false;
                return true;
            }
            Require(Contains(new Vector3(0f, .2f, 0f)) && !Contains(new Vector3(0f, .01f, 0f))
                && !Contains(new Vector3(0f, .4f, -2.6f)) && !Contains(new Vector3(0f, .4f, 2.65f)),
                "interior excludes water while keel and rudder regions retain water", report);
            foreach (var vertex in volume.Boundary.vertices)
                foreach (var plane in volume.Planes)
                    Require(Vector3.Dot(new Vector3(plane.x, plane.y, plane.z), vertex) + plane.w < .003f,
                        "closed boundary stays within its clipping planes", report);
            report.AppendLine($"Source mesh: {source.sharedMesh.bounds}; selected shell: {shell}; fitted interior: {bounds}");
            report.AppendLine($"Planes: {volume.Planes.Length}; boundary vertices: {volume.Boundary.vertexCount}");
            report.AppendLine("SUCCESS: Rowing boat main-shell interior fit passed.");
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); Debug.LogException(error); }
        finally
        {
            PrefabUtility.UnloadPrefabContents(root);
            File.AppendAllText(ReportPath, report.ToString());
            Debug.Log(report.ToString());
        }
    }

    private static void Require(bool condition, string message, StringBuilder report)
    {
        if (!condition) throw new InvalidOperationException(message);
        string line = "PASS: " + message;
        if (!report.ToString().Contains(line)) report.AppendLine(line);
    }
}
