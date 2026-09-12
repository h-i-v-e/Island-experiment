using Motu.Rendering;
using Motu.World;
using UnityEngine;

namespace Motu.Gameplay
{
    public sealed partial class IslandDemoController
    {
        private bool awayFromHelm;
        public bool IsAtHelm => shipController != null && !awayFromHelm;
        public KeyCode SwitchCameraKey => switchCameraKey;

        /// <summary>Leave the ship floating, or return to its current bridge pose.</summary>
        public bool ToggleShipCamera()
        {
            if (shipController == null || shipController.BridgeCamera == null
                || firstPersonController == null || worldManager == null
                || viewerCamera == null || orbitCamera == null) return false;
            if (!IsAtHelm) return ReturnToHelm();
            var position = viewerCamera.transform.position;
            var angles = viewerCamera.transform.eulerAngles;
            if (!UseExplorationCamera()) return false;
            var pitch = Mathf.DeltaAngle(0, angles.x);
            // Keep the bridge eye height instead of snapping into the hull at
            // the terrain-following flight mode's sea-level clearance.
            firstPersonController.BeginFlying(position, angles.y, pitch, followTerrain: false);
            return true;
        }

        /// <summary>Teleport the player independently of the ship, including from the helm.</summary>
        public bool TeleportPlayer(Vector3 destination, bool releaseCursor = true)
        {
            if (firstPersonController == null) return false;
            if (IsAtHelm && !ToggleShipCamera()) return false;
            firstPersonController.Teleport(destination, releaseCursor);
            RemoveShipOutsidePlayerCell();
            return true;
        }

        private void LateUpdate() => RemoveShipOutsidePlayerCell();

        internal void RemoveShipOutsidePlayerCell()
        {
            if (IsAtHelm || shipController == null || viewerCamera == null) return;
            if (IslandWorldManager.WorldToCell(viewerCamera.transform.position)
                == IslandWorldManager.WorldToCell(shipController.transform.position)) return;

            var ship = shipController.gameObject;
            var bridge = shipController.BridgeCamera;
            shipController = null;
            // Deactivate first so probes and wave stamps unregister immediately.
            // The separately rooted bridge camera belongs to the departing ship.
            ship.SetActive(false);
            if (bridge != null && !bridge.transform.IsChildOf(ship.transform))
            {
                bridge.gameObject.SetActive(false);
                UnityObjectLifetime.DestroyUnityObject(bridge.gameObject);
            }
            UnityObjectLifetime.DestroyUnityObject(ship);
        }

        private bool UseExplorationCamera()
        {
            if (firstPersonController == null || worldManager == null || orbitCamera == null) return false;
            var camera = firstPersonController.GetComponent<Camera>();
            if (camera == null) return false;
            if (shipController != null)
            {
                shipController.enabled = false;
                if (shipController.BridgeCamera != null) shipController.BridgeCamera.enabled = false;
                awayFromHelm = true;
            }
            orbitCamera.Configure(viewerCamera != null ? viewerCamera.transform.position : camera.transform.position, 120f);
            firstPersonController.Configure(orbitCamera, worldManager);
            SetViewerCamera(camera);
            clickCandidate = minimapClickCandidate = false;
            return true;
        }

        private bool ReturnToHelm()
        {
            var bridge = shipController.BridgeCamera;
            var camera = bridge.GetComponent<Camera>();
            if (camera == null) return false;
            firstPersonController.Exit();
            firstPersonController.enabled = false;
            orbitCamera.enabled = false;
            SetViewerCamera(camera);
            awayFromHelm = false;
            bridge.enabled = true;
            bridge.SnapToShip();
            shipController.enabled = true;
            worldManager.SetStreamingTarget(camera.transform);
            worldManager.SetFirstPersonViewActive(true);
            bridge.SetMouseLook(true);
            clickCandidate = minimapClickCandidate = false;
            return true;
        }

        private void SetViewerCamera(Camera camera)
        {
            if (viewerCamera == camera) return;
            if (viewerCamera != null)
            {
                TransferEffect<PlanarWaterReflection>(viewerCamera, camera);
                TransferEffect<RealTimeAmbientOcclusion>(viewerCamera, camera);
                if (viewerCamera.GetComponent<OceanUnderwaterView>() != null
                    && camera.GetComponent<OceanUnderwaterView>() == null)
                    camera.gameObject.AddComponent<OceanUnderwaterView>();
                TransferEffect<OceanUnderwaterView>(viewerCamera, camera);
                viewerCamera.enabled = false;
                if (viewerCamera.CompareTag("MainCamera")) viewerCamera.tag = "Untagged";
                SetListenerEnabled(viewerCamera, false);
            }
            viewerCamera = camera;
            viewerCamera.tag = "MainCamera";
            viewerCamera.enabled = true;
            SetListenerEnabled(viewerCamera, true);
        }

        private static void TransferEffect<T>(Camera previous, Camera next) where T : Behaviour
        {
            var source = previous.GetComponent<T>();
            var destination = next.GetComponent<T>();
            if (destination != null) destination.enabled = source != null && source.enabled;
            if (source != null) source.enabled = false;
        }

        private static void SetListenerEnabled(Camera camera, bool active)
        {
            // AudioModule is optional in this project.
            var listener = camera.GetComponent("AudioListener") as Behaviour;
            if (listener != null) listener.enabled = active;
        }
    }
}
