using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandDebugSettings
{
    [Tooltip("Draw the generated terrain mesh edges in black over its normal materials.")]
    [SerializeField] private bool showMeshEdges;

    [Tooltip(
        "Key used in Play Mode to toggle the terrain mesh-edge overlay. "
        + "Set to None to disable the shortcut.")]
    [SerializeField] private KeyCode toggleMeshEdgesKey = KeyCode.M;

    [Tooltip("Draw tree triangle edges over the normal wood and foliage materials.")]
    [SerializeField] private bool showTreeMeshEdges;

    [Tooltip(
        "Key used in Play Mode to toggle the tree-only mesh-edge overlay. "
        + "Set to None to disable the shortcut.")]
    [SerializeField] private KeyCode toggleTreeMeshEdgesKey = KeyCode.N;

    [Tooltip("Display a smoothed frame-rate counter in the top-right corner.")]
    [SerializeField] private bool showFrameRate;

    [Tooltip(
        "Key used in Play Mode to toggle the frame-rate counter. "
        + "Set to None to disable the shortcut.")]
    [SerializeField] private KeyCode toggleFrameRateKey = KeyCode.F;

    [Tooltip("Display authoritative waterfall-foot fog-volume markers.")]
    [FormerlySerializedAs("showRoughWaterEmitters")]
    [SerializeField] private bool showWaterfallFeet;

    public bool ShowMeshEdges { get => showMeshEdges; set => showMeshEdges = value; }
    public KeyCode ToggleMeshEdgesKey => toggleMeshEdgesKey;
    public bool ShowTreeMeshEdges { get => showTreeMeshEdges; set => showTreeMeshEdges = value; }
    public KeyCode ToggleTreeMeshEdgesKey => toggleTreeMeshEdgesKey;
    public bool ShowFrameRate { get => showFrameRate; set => showFrameRate = value; }
    public KeyCode ToggleFrameRateKey => toggleFrameRateKey;
    public bool ShowWaterfallFeet { get => showWaterfallFeet; set => showWaterfallFeet = value; }
}
