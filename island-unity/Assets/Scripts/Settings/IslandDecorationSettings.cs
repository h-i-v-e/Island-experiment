using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandDecorationSettings
{
    [Tooltip("Tree prefabs reserved for the vegetation placement phase.")]
    [SerializeField] private GameObject[] treePrefabs = Array.Empty<GameObject>();

    [Tooltip("Plant and shrub prefabs reserved for the vegetation placement phase.")]
    [SerializeField] private GameObject[] plantPrefabs = Array.Empty<GameObject>();

    public GameObject[] TreePrefabs => treePrefabs;
    public GameObject[] PlantPrefabs => plantPrefabs;
}
