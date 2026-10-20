using System;
using System.IO;
using System.Linq;
using System.Text;
using Motu.Gameplay;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;

public static class ExplorerRowboatLaunchSetup
{
    public const string ExplorerPath = "Assets/Prefabs/ExplorerShip.prefab";
    [MenuItem("Island/Boat Tests/Set Up Explorer Deck Rowboat Launches")]
    public static void Apply()
    {
        if (EditorApplication.isPlayingOrWillChangePlaymode) throw new InvalidOperationException("Stop Play mode first.");
        var report = new StringBuilder();
        var root = PrefabUtility.LoadPrefabContents(ExplorerPath);
        try { Configure(root, false, report); PrefabUtility.SaveAsPrefabAsset(root, ExplorerPath); }
        finally { PrefabUtility.UnloadPrefabContents(root); }
        for (int i = 0; i < SceneManager.sceneCount; i++)
        {
            var scene = SceneManager.GetSceneAt(i);
            if (!scene.isLoaded || EditorSceneManager.IsPreviewScene(scene)) continue;
            foreach (var sceneRoot in scene.GetRootGameObjects())
                foreach (var boat in sceneRoot.GetComponentsInChildren<FloatingShip>(true))
                    if (PrefabUtility.GetPrefabAssetPathOfNearestInstanceRoot(boat.gameObject) == ExplorerPath)
                    {
                        Undo.RegisterFullObjectHierarchyUndo(boat.gameObject, "Set up Explorer rowboat launches");
                        Configure(boat.gameObject, true, report);
                        EditorSceneManager.MarkSceneDirty(scene);
                    }
        }
        report.AppendLine("Deck geometry and existing transforms preserved; prefab updated and open scene left unsaved.");
        File.WriteAllText("/tmp/explorer-rowboat-launch-setup.txt", report.ToString());
        Debug.Log(report.ToString());
    }

    static void Configure(GameObject root, bool undo, StringBuilder report)
    {
        var ship = root.GetComponentInChildren<ShipController>(true);
        var boat = AssetDatabase.LoadAssetAtPath<GameObject>(BoatTestSceneSetup.BoatPrefab);
        var npc = AssetDatabase.LoadAssetAtPath<GameObject>(NpcPrefabSetup.PrefabPath);
        if (ship == null || boat == null || npc == null) throw new InvalidOperationException("Explorer, floating rowboat and NPC prefab are required.");
        var models = root.GetComponentsInChildren<MeshFilter>(true).Where(m => m.name.StartsWith("SM_Veh_Boat_Rowing_01_Hull")).ToArray();
        if (models.Length != 2) throw new InvalidOperationException("Expected the two deck rowboat hulls; found " + models.Length);
        var hull = ship.GetComponentsInChildren<MeshCollider>(true).First(c => c.sharedMesh != null && c.attachedRigidbody == ship.GetComponent<Rigidbody>());
        var shipBounds = InFrame(root.transform, hull.transform, hull.sharedMesh.bounds);
        Bounds span = default;
        bool first = true;
        foreach (var mesh in boat.GetComponentsInChildren<MeshFilter>(true))
        {
            var b = InFrame(boat.transform, mesh.transform, mesh.sharedMesh.bounds);
            if (first) { span = b; first = false; } else span.Encapsulate(b);
        }
        var boatHull = boat.GetComponentsInChildren<MeshCollider>(true).First(c => c.convex);
        var clearance = InFrame(boat.transform, boatHull.transform, boatHull.sharedMesh.bounds);
        var group = root.transform.Find("Rowboat launch points");
        if (group == null)
        {
            group = new GameObject("Rowboat launch points").transform;
            group.SetParent(root.transform, false);
            if (undo) Undo.RegisterCreatedObjectUndo(group.gameObject, "Add launch points");
        }
        foreach (var model in models)
        {
            float side = Mathf.Sign(root.transform.InverseTransformPoint(model.transform.position).x);
            string label = side < 0 ? "Port launch" : "Starboard launch";
            var point = group.Find(label);
            if (point == null)
            {
                point = new GameObject(label).transform;
                point.SetParent(group, false);
                float shipEdge = side < 0 ? -shipBounds.min.x : shipBounds.max.x;
                float oarReach = side < 0 ? span.max.x : -span.min.x;
                point.localPosition = new Vector3(side * (shipEdge + oarReach + .5f), 0,
                    root.transform.InverseTransformPoint(model.transform.position).z);
                if (undo) Undo.RegisterCreatedObjectUndo(point.gameObject, "Add launch point");
            }
            var launch = model.GetComponent<ShipRowboatLaunch>();
            if (launch == null) launch = undo ? Undo.AddComponent<ShipRowboatLaunch>(model.gameObject) : model.gameObject.AddComponent<ShipRowboatLaunch>();
            var data = new SerializedObject(launch);
            data.FindProperty("ship").objectReferenceValue = ship;
            data.FindProperty("shipFrame").objectReferenceValue = root.transform;
            data.FindProperty("stowedHull").objectReferenceValue = model.GetComponent<Renderer>();
            data.FindProperty("launchPoint").objectReferenceValue = point;
            data.FindProperty("rowboatPrefab").objectReferenceValue = boat;
            data.FindProperty("rowerPrefab").objectReferenceValue = npc;
            data.FindProperty("clearanceBounds").boundsValue = clearance;
            data.ApplyModifiedPropertiesWithoutUndo();
            if (PrefabUtility.IsPartOfPrefabInstance(launch)) PrefabUtility.RecordPrefabInstancePropertyModifications(launch);
            report.AppendLine($"{label}: deck hull at {root.transform.InverseTransformPoint(model.transform.position):F3}; launch at {point.localPosition:F3}; hull-only clearance {clearance}.");
        }
    }

    public static Bounds InFrame(Transform frame, Transform owner, Bounds bounds)
    {
        Bounds result = default;
        for (int i = 0; i < 8; i++)
        {
            var corner = bounds.center + Vector3.Scale(bounds.extents, new Vector3((i & 1) == 0 ? -1 : 1, (i & 2) == 0 ? -1 : 1, (i & 4) == 0 ? -1 : 1));
            var point = frame.InverseTransformPoint(owner.TransformPoint(corner));
            if (i == 0) result = new Bounds(point, Vector3.zero); else result.Encapsulate(point);
        }
        return result;
    }
}
