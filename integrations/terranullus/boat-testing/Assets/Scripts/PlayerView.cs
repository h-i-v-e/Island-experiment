using Motu.Gameplay;
using Motu.World;
using UnityEngine;

public static class PlayerView
{
    // Transfer both the rendered view and world streaming from the ship bridge
    // to the actual character, for helm use and rowboat launch alike.
    public static Camera Activate(PlayerController player)
    {
        if (player == null || !player.UsesPlayerInput) return null;
        var view = player.GetComponentInChildren<Camera>(true);
        if (view == null) return null;
        foreach (var bridge in Object.FindObjectsByType<ShipBridgeCamera>(FindObjectsSortMode.None))
        {
            var camera = bridge.GetComponent<Camera>();
            if (camera != null && camera != view)
            {
                view.farClipPlane = Mathf.Max(view.farClipPlane, camera.farClipPlane);
                if (camera.CompareTag("MainCamera")) camera.tag = "Untagged";
                camera.enabled = false;
            }
            bridge.enabled = false;
        }
        view.enabled = true;
        view.tag = "MainCamera";
        Cursor.lockState = CursorLockMode.Locked;
        Cursor.visible = false;
        var world = Object.FindAnyObjectByType<IslandWorldManager>();
        if (world != null)
        {
            world.SetStreamingTarget(view.transform);
            world.SetFirstPersonViewActive(true);
        }
        return view;
    }
}
