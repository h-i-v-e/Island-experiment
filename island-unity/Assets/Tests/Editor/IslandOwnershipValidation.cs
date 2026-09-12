using System;
using UnityEngine;
using Motu.Islands;

namespace Motu.Editor
{
    internal static class IslandOwnershipValidation
    {

        internal static void ValidateOwnershipContract()
        {
            var firstParent = new GameObject("First island runtime validation parent");
            var secondParent = new GameObject("Second island runtime validation parent");
            firstParent.transform.position = new Vector3(1200f, 0f, -600f);
            secondParent.transform.position = new Vector3(-900f, 0f, 1700f);
            var first = IslandRuntime.Create(
                new IslandDescriptor(
                    "validation-a",
                    Vector2Int.zero,
                    1200d,
                    -600d,
                    11,
                    1000f,
                    1),
                firstParent.transform);
            var second = IslandRuntime.Create(
                new IslandDescriptor(
                    "validation-b",
                    Vector2Int.one,
                    -900d,
                    1700d,
                    12,
                    1000f,
                    1),
                secondParent.transform);
            var shader = Shader.Find("Hidden/Motu/Ocean Wave Attenuation")
                ?? throw new InvalidOperationException(
                    "The coastal mask shader is unavailable for island-runtime validation.");
            var firstMaterial = new Material(shader);
            var secondMaterial = new Material(shader);
            var firstMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            var secondMask = new Texture2D(1, 1, TextureFormat.RGBA32, false, true);
            var firstRoot = first.gameObject;
            var secondRoot = second.gameObject;
            try
            {
                firstMaterial.SetMatrix("_IslandWorldToLocal", first.transform.worldToLocalMatrix);
                secondMaterial.SetMatrix("_IslandWorldToLocal", second.transform.worldToLocalMatrix);
                firstMaterial.SetTexture("_SeaMask", firstMask);
                secondMaterial.SetTexture("_SeaMask", secondMask);
                first.OwnMaterial(firstMaterial);
                second.OwnMaterial(secondMaterial);
                first.OwnTexture(firstMask);
                second.OwnTexture(secondMask);
                if (first.transform.position == second.transform.position
                    || firstMaterial.GetMatrix("_IslandWorldToLocal")
                        == secondMaterial.GetMatrix("_IslandWorldToLocal")
                    || firstMaterial.GetTexture("_SeaMask")
                        == secondMaterial.GetTexture("_SeaMask"))
                {
                    throw new InvalidOperationException(
                        "Two island runtimes contaminated their transforms or coast masks.");
                }
            }
            finally
            {
                first.Dispose();
                second.Dispose();
                UnityEngine.Object.DestroyImmediate(firstParent);
                UnityEngine.Object.DestroyImmediate(secondParent);
            }
            if (firstRoot != null
                || secondRoot != null
                || firstMaterial != null
                || secondMaterial != null
                || firstMask != null
                || secondMask != null)
            {
                throw new InvalidOperationException(
                    "Island runtime disposal did not release its owned Unity objects.");
            }
        }
    }
}
