using System;
using UnityEngine;
using UnityEngine.Serialization;

[Serializable]
public sealed class IslandStreamingSettings
{
    [Tooltip("Player or camera Transform that drives terrain detail, collision, rocks, grass, and river effects.")]
    [SerializeField] private Transform target;

    public Transform Target { get => target; set => target = value; }
}
