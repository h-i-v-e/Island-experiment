using System;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

public static class IslandGeneratorValidation
{
    private const string SandboxScenePath = "Assets/Scenes/IslandSandbox.unity";

    public static void BatchValidateNativeInterop()
    {
        IslandGenerator.BatchValidateNativeInterop();
        ValidateSandboxScene();
        Debug.Log("IslandGenerator component, sandbox level, and native validation passed.");
    }

    private static void ValidateSandboxScene()
    {
        var scene = EditorSceneManager.OpenScene(SandboxScenePath);
        if (!scene.IsValid())
        {
            throw new InvalidOperationException("The island sandbox scene could not be opened.");
        }
        var islands = UnityEngine.Object.FindObjectsByType<IslandGenerator>(
            FindObjectsInactive.Include);
        if (islands.Length != 1)
        {
            throw new InvalidOperationException(
                $"The sandbox scene must contain exactly one IslandGenerator; found {islands.Length}.");
        }
        var island = islands[0];
        if (island.Streaming.Target == null)
        {
            throw new InvalidOperationException(
                "The sandbox IslandGenerator has no streaming target.");
        }
        if (island.Rendering.TerrainMaterial == null
            || island.Rendering.GrassMaterial == null
            || island.Rendering.RiverMaterial == null
            || island.Rendering.SeaMaterial == null
            || island.Rendering.RockMaterial == null)
        {
            throw new InvalidOperationException(
                "The sandbox IslandGenerator is missing a default material template.");
        }
        if (island.Rendering.RiverMaterial.shader.name != "Motu/River Water"
            || island.Rendering.SeaMaterial.shader.name != "Motu/Sea Water")
        {
            throw new InvalidOperationException(
                "The sandbox river and sea materials do not use their dedicated shaders.");
        }
        if (island.Decorations.TreePrefabs == null
            || island.Decorations.PlantPrefabs == null
            || island.Decorations.StoneAndBoulderPrefabs == null)
        {
            throw new InvalidOperationException(
                "The sandbox decoration asset libraries are not serialized.");
        }
        var sceneEnabled = EditorBuildSettings.scenes.Any(
            entry => entry.enabled && entry.path == SandboxScenePath);
        if (!sceneEnabled)
        {
            throw new InvalidOperationException(
                "The island sandbox scene is not enabled in Build Settings.");
        }

        var originalPosition = island.transform.position;
        var originalRotation = island.transform.rotation;
        island.transform.SetPositionAndRotation(
            new Vector3(120f, 15f, -80f),
            Quaternion.Euler(0f, 37f, 0f));
        var local = new Vector3(31f, 7f, -19f);
        var roundTrip = island.transform.InverseTransformPoint(
            island.transform.TransformPoint(local));
        island.transform.SetPositionAndRotation(originalPosition, originalRotation);
        if ((roundTrip - local).sqrMagnitude > 1.0e-6f)
        {
            throw new InvalidOperationException(
                "Island local/world transform conversion failed validation.");
        }
    }
}
