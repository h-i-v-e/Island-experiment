using System;
using System.IO;
using System.Reflection;
using System.Text;
using Motu.Gameplay;
using Motu.Islands;
using Motu.World;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.SceneManagement;
using Object = UnityEngine.Object;

[InitializeOnLoad]
public static class UnattendedShipValidation
{
    const string Key = "UnattendedShipValidation";
    const string Report = "/tmp/unattended-ship-validation.txt";
    const BindingFlags Private = BindingFlags.Instance | BindingFlags.NonPublic;

    static UnattendedShipValidation()
    {
        EditorApplication.playModeStateChanged += state =>
        {
            if (!SessionState.GetBool(Key, false)) return;
            if (state == PlayModeStateChange.EnteredPlayMode) Validate();
            if (state == PlayModeStateChange.EnteredEditMode)
            {
                SessionState.SetBool(Key, false);
                EditorApplication.Exit(File.ReadAllText(Report).Contains("FAIL:") ? 1 : 0);
            }
        };
    }

    public static void Run()
    {
        if (!Application.isBatchMode || !File.Exists(".boat-exit-isolated-project"))
            throw new InvalidOperationException("Run in the isolated boat validation project only.");
        EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
        SessionState.SetBool(Key, true);
        EditorApplication.EnterPlaymode();
    }

    static void Validate()
    {
        var report = new StringBuilder();
        var scene = SceneManager.CreateScene("Unattended ship physics", new CreateSceneParameters(LocalPhysicsMode.Physics3D));
        GameObject root = null, worldRoot = new GameObject("Read-only world fixture");
        try
        {
            worldRoot.SetActive(false);
            var factory = worldRoot.AddComponent<CoherentIslandFactory>();
            Set(factory, "islandThreshold", 0f); // Origin is sea; the adjoining cell always contains an island.
            var world = worldRoot.AddComponent<IslandWorldManager>();
            world.ConfigureIslandGenerationRequestFactory(factory);
            root = (GameObject)PrefabUtility.InstantiatePrefab(AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/ExplorerShip.prefab"), scene);
            var floating = root.GetComponent<FloatingShip>();
            var body = floating.Hull;
            var ship = body.GetComponent<ShipController>();
            var anchor = body.GetComponent<ShipAnchor>();
            body.GetComponent<OceanBuoyancy>().enabled = false; // This fixture exercises horizontal native physics without GPU waves.
            body.useGravity = false;
            body.isKinematic = false;
            body.constraints = RigidbodyConstraints.FreezePositionY | RigidbodyConstraints.FreezeRotationX | RigidbodyConstraints.FreezeRotationZ;
            body.position = new Vector3(0, -3, 0);
            body.rotation = Quaternion.identity;
            body.transform.SetPositionAndRotation(body.position, body.rotation);
            Set(floating, "world", world);
            Set(floating, "nextWorldSearch", float.PositiveInfinity);
            Set(floating, "players", Array.Empty<PlayerController>());
            Call(floating, "CacheHullColliders");
            Call(ship, "CacheBody");
            ship.SailsRaised = false;
            Check(anchor.IsDeployed && !world.HasActiveTerrainColliderAt(body.position), "Furled ship is unattended in an open-sea cell with no terrain collider", report);
            Call(floating, "UpdateTerrainSuspension");
            Check(!body.isKinematic, "Leaving the ship in known open sea keeps its physics dynamic", report);

            // Give the actual anchor a native seabed and slack rode, then turn the ship in wind.
            var terrainData = new TerrainData { heightmapResolution = 33, size = new Vector3(500, 1, 500) };
            var bottom = Terrain.CreateTerrainGameObject(terrainData);
            SceneManager.MoveGameObjectToScene(bottom, scene);
            bottom.transform.position = new Vector3(-250, -8, -250);
            Physics.SyncTransforms();
            anchor.ResetAfterTeleport();
            Check(anchor.IsHolding, "The real bow anchor catches the fixture seabed", report);
            float initialError = WindError(anchor, body, Vector3.right * 9);
            Advance(floating, anchor, scene.GetPhysicsScene(), Vector3.right * 9, 20);
            float firstError = WindError(anchor, body, Vector3.right * 9);
            Check(!body.isKinematic && firstError < initialError - 5, $"Unattended anchored ship turns towards wind source: {initialError:F1} to {firstError:F1} degrees", report);
            float changedError = WindError(anchor, body, Vector3.forward * 9);
            Advance(floating, anchor, scene.GetPhysicsScene(), Vector3.forward * 9, 30);
            Check(WindError(anchor, body, Vector3.forward * 9) < changedError - 5,
                "Unattended anchored ship responds to a later wind shift", report);
            Check(Vector3.Distance(anchor.BowWorldPosition, anchor.AnchorPosition) < anchor.RodeLengthMetres + .5f,
                "Native rode still limits the anchored bow's swing", report);
            Object.DestroyImmediate(bottom);
            Object.DestroyImmediate(terrainData);
            anchor.SetDeployed(false);

            // Check the entire hull, including its corners across a sea/island cell boundary.
            body.position = new Vector3(995, -3, 0);
            body.rotation = Quaternion.identity;
            body.transform.SetPositionAndRotation(body.position, body.rotation);
            body.linearVelocity = new Vector3(1, 0, 2);
            body.angularVelocity = Vector3.up * .1f;
            Physics.SyncTransforms();
            Call(floating, "UpdateTerrainSuspension");
            Check(body.isKinematic, "Hull overlapping an island cell with unavailable collision still suspends", report);
            var stoppedPosition = body.position;
            scene.GetPhysicsScene().Simulate(.02f);
            Check(body.position == stoppedPosition, "Suspension prevents travelling into unloaded shoreline", report);
            body.position = new Vector3(0, -3, 0);
            body.transform.position = body.position;
            Call(floating, "UpdateTerrainSuspension");
            Check(!body.isKinematic && Vector3.Distance(body.linearVelocity, new Vector3(1, 0, 2)) < .001f
                && Mathf.Abs(body.angularVelocity.y - .1f) < .001f, "Returning to open sea resumes the saved drift and yaw", report);
            report.AppendLine("SUCCESS: Unattended ship wind, anchor and terrain safety passed.");
        }
        catch (Exception error) { report.AppendLine("FAIL: " + error); Debug.LogException(error); }
        finally
        {
            if (root != null) Object.DestroyImmediate(root);
            Object.DestroyImmediate(worldRoot);
            SceneManager.UnloadSceneAsync(scene);
            File.WriteAllText(Report, report.ToString());
            Debug.Log(report.ToString());
            EditorApplication.ExitPlaymode();
        }
    }

    static void Advance(FloatingShip floating, ShipAnchor anchor, PhysicsScene physics, Vector3 wind, float seconds)
    {
        for (int i = 0; i < seconds / .02f; i++)
        {
            Call(floating, "UpdateTerrainSuspension");
            typeof(ShipAnchor).GetMethod("Simulate", Private).Invoke(anchor, new object[] { wind });
            physics.Simulate(.02f);
        }
    }

    static float WindError(ShipAnchor anchor, Rigidbody body, Vector3 wind) =>
        Vector3.Angle(Vector3.ProjectOnPlane(anchor.BowWorldPosition - body.worldCenterOfMass, Vector3.up), -wind);
    static void Set(object target, string name, object value) => target.GetType().GetField(name, Private).SetValue(target, value);
    static void Call(object target, string name) => target.GetType().GetMethod(name, Private).Invoke(target, null);
    static void Check(bool condition, string message, StringBuilder report)
    {
        if (!condition) throw new InvalidOperationException(message);
        report.AppendLine("PASS: " + message);
    }
}
