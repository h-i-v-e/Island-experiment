using System;
using Motu.Rendering;
using UnityEditor;
using UnityEngine;

namespace Motu.Editor
{
    public sealed class ShipWaveTextureWindow : EditorWindow
    {
        [SerializeField] private GameObject ship;
        [SerializeField] private Texture2D hullTexture;
        [SerializeField] private Texture2D wakeTexture;
        private string message;

        public static void Open()
        {
            var window = GetWindow<ShipWaveTextureWindow>("Ship Wave Textures");
            window.minSize = new Vector2(420, 280);
            window.UseSelection();
            window.Show();
        }

        private void UseSelection()
        {
            var selected = Selection.activeGameObject;
            if (selected == null) { LoadShip(null); return; }
            var component = selected.GetComponentInParent<OceanDeckWaveClamp>();
            var body = selected.GetComponentInParent<Rigidbody>();
            LoadShip(component != null ? component.gameObject : body != null ? body.gameObject : selected);
        }

        internal void LoadShip(GameObject target)
        {
            ship = target;
            var component = ship != null ? ship.GetComponent<OceanDeckWaveClamp>() : null;
            hullTexture = component != null ? component.HullTexture : null;
            wakeTexture = component != null ? component.WakeTexture : null;
            message = null;
        }

        private void OnGUI()
        {
            EditorGUILayout.HelpBox("Choose the hull and wake textures for this ship. Save them on each ship prefab to reuse that type's setup. Existing shape, dimensions and wave tuning are preserved.", MessageType.Info);
            if (GUILayout.Button("Use Selected Ship")) UseSelection();
            EditorGUI.BeginChangeCheck();
            var target = (GameObject)EditorGUILayout.ObjectField("Ship", ship, typeof(GameObject), true);
            if (EditorGUI.EndChangeCheck()) LoadShip(target);
            hullTexture = (Texture2D)EditorGUILayout.ObjectField("Hull Wave Texture", hullTexture, typeof(Texture2D), false);
            wakeTexture = (Texture2D)EditorGUILayout.ObjectField("Wake Texture", wakeTexture, typeof(Texture2D), false);
            EditorGUILayout.LabelField("No hull texture: capsule fallback. No wake texture: no trailing wake.", EditorStyles.wordWrappedLabel);
            var persistent = ship != null && EditorUtility.IsPersistent(ship);
            if (persistent) EditorGUILayout.HelpBox("Open this prefab in Prefab Mode to apply its texture setup.", MessageType.Info);
            if (EditorApplication.isPlaying) EditorGUILayout.HelpBox("Exit Play Mode to save texture assignments.", MessageType.Info);
            using (new EditorGUI.DisabledScope(ship == null || persistent || EditorApplication.isPlaying))
                if (GUILayout.Button("Apply Textures To This Ship"))
                {
                    try
                    {
                        ShipDeckWaveClampSetup.ApplyTextures(ship, hullTexture, wakeTexture);
                        LoadShip(ship);
                        message = "Textures applied. Save the scene or prefab to keep the assignment. Imported images are configured as linear, uncompressed 2D data.";
                    }
                    catch (Exception exception) { message = exception.Message; }
                }
            if (!string.IsNullOrEmpty(message)) EditorGUILayout.HelpBox(message, MessageType.Info);
        }
    }
}
