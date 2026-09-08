using System;
using Motu.Gameplay;
using Motu.Rendering;
using Motu.World;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Motu.Editor
{
    public static class ShipHelmSetup
    {
        [MenuItem("Island/Set Up Selected Ship Helm")]
        public static void ConfigureSelectedShip()
        {
            if (EditorApplication.isPlaying) throw new InvalidOperationException("Set up the helm outside Play Mode.");
            var body = Selection.activeGameObject != null ? Selection.activeGameObject.GetComponentInParent<Rigidbody>() : null;
            if (body == null) throw new InvalidOperationException("Select the ship's Rigidbody first.");
            Configure(body);
        }

        internal static ShipController Configure(Rigidbody body)
        {
            var manager = Object.FindAnyObjectByType<IslandWorldManager>();
            var demo = Object.FindAnyObjectByType<IslandDemoController>();
            if (manager == null || demo == null) throw new InvalidOperationException("Open the world scene before setting up its ship helm.");
            var oldCamera = demo.GetComponent<Camera>();
            var mesh = body.GetComponentInChildren<MeshFilter>();
            if (mesh == null || mesh.sharedMesh == null) throw new InvalidOperationException("The ship has no hull mesh.");
            Undo.SetCurrentGroupName("Set up ship helm");
            var controller = body.GetComponent<ShipController>() ?? Undo.AddComponent<ShipController>(body.gameObject);
            var buoyancy = body.GetComponent<OceanBuoyancy>();
            Undo.RecordObject(buoyancy, "Set cruising water drag");
            var buoyancySettings = new SerializedObject(buoyancy);
            buoyancySettings.FindProperty("waterDrag").floatValue = .08f;
            buoyancySettings.ApplyModifiedProperties();

            // This imported pirate ship has its bow along +X and deck-up along Z.
            // The reference is editable independently of the FBX's root rotation.
            var forward = body.transform.TransformDirection(Vector3.right).normalized;
            var up = body.transform.TransformDirection(Vector3.forward).normalized;
            var deck = FindBridgeDeck(mesh, forward, up);
            var eye = body.transform.Find("Bridge Eye");
            if (eye == null)
            {
                var eyeObject = new GameObject("Bridge Eye");
                Undo.RegisterCreatedObjectUndo(eyeObject, "Create bridge eye position");
                eye = eyeObject.transform;
                Undo.SetTransformParent(eye, body.transform, "Parent bridge eye");
            }
            Undo.RecordObject(eye, "Place bridge eye");
            eye.SetPositionAndRotation(deck + up * 1.7f, Quaternion.LookRotation(forward, up));
            var scale = body.transform.lossyScale;
            eye.localScale = new Vector3(1f / scale.x, 1f / scale.y, 1f / scale.z);

            var rig = controller.BridgeCamera;
            if (rig == null)
            {
                var cameraObject = new GameObject("Ship Bridge Camera");
                Undo.RegisterCreatedObjectUndo(cameraObject, "Create bridge camera");
                var camera = Undo.AddComponent<Camera>(cameraObject);
                if (oldCamera != null) camera.CopyFrom(oldCamera);
                camera.enabled = true;
                camera.nearClipPlane = .08f;
                camera.fieldOfView = 75f;
                cameraObject.tag = "MainCamera";
                rig = Undo.AddComponent<ShipBridgeCamera>(cameraObject);
                var reflection = oldCamera != null ? oldCamera.GetComponent<PlanarWaterReflection>() : null;
                if (reflection != null) EditorUtility.CopySerialized(reflection, Undo.AddComponent<PlanarWaterReflection>(cameraObject));
                var ao = oldCamera != null ? oldCamera.GetComponent<RealTimeAmbientOcclusion>() : null;
                if (ao != null) EditorUtility.CopySerialized(ao, Undo.AddComponent<RealTimeAmbientOcclusion>(cameraObject));
            }
            Undo.RecordObject(rig, "Configure bridge camera");
            rig.Configure(eye, body);
            Undo.RecordObject(controller, "Configure ship controls");
            controller.Configure(eye, rig, manager);
            controller.enabled = rig.enabled = true;
            Undo.RecordObject(demo, "Start at ship helm");
            demo.ConfigureShipStart(controller, rig.GetComponent<Camera>());
            var managerSettings = new SerializedObject(manager);
            managerSettings.FindProperty("streamingTarget").objectReferenceValue = rig.transform;
            managerSettings.ApplyModifiedProperties();
            if (oldCamera != null)
            {
                Undo.RecordObject(oldCamera.gameObject, "Retire overview camera tag");
                oldCamera.gameObject.tag = "Untagged";
                foreach (var component in oldCamera.GetComponents<Behaviour>())
                {
                    if (component == demo) continue;
                    Undo.RecordObject(component, "Disable previous camera controls");
                    component.enabled = false;
                }
            }
            PrefabUtility.RecordPrefabInstancePropertyModifications(body);
            PrefabUtility.RecordPrefabInstancePropertyModifications(buoyancy);
            PrefabUtility.RecordPrefabInstancePropertyModifications(controller);
            EditorSceneManager.MarkSceneDirty(body.gameObject.scene);
            Selection.activeGameObject = rig.gameObject;
            Debug.Log($"Ship helm configured: bridge deck {deck}, eye {eye.position}, forward {forward}; camera is 1.7 m above the deck.");
            return controller;
        }

        private static Vector3 FindBridgeDeck(MeshFilter source, Vector3 forward, Vector3 up)
        {
            var bounds = source.GetComponent<Renderer>().bounds;
            var along = Mathf.Abs(forward.x) * bounds.size.x + Mathf.Abs(forward.z) * bounds.size.z;
            // Stand slightly to starboard so the aft mast does not block the
            // initial view straight down the deck towards the bow.
            var origin = bounds.center - forward * (along * .36f) + Vector3.Cross(up, forward) * 1.2f;
            origin.y = bounds.max.y + 5f;
            var temporary = new GameObject("Bridge placement query") { hideFlags = HideFlags.HideAndDontSave };
            try
            {
                temporary.transform.SetPositionAndRotation(source.transform.position, source.transform.rotation);
                temporary.transform.localScale = source.transform.lossyScale;
                var collider = temporary.AddComponent<MeshCollider>();
                collider.sharedMesh = source.sharedMesh;
                Physics.SyncTransforms();
                if (!collider.Raycast(new Ray(origin, -up), out var hit, bounds.size.y + 10f))
                    throw new InvalidOperationException("Could not locate the aft deck on the ship mesh.");
                return hit.point;
            }
            finally { Object.DestroyImmediate(temporary); }
        }
    }
}
