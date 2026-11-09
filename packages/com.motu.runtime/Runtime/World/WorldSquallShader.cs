using UnityEngine;

namespace Motu.World
{
    internal static class WorldSquallShader
    {
        // Keep the first pair compatible with existing single-cell materials.
        internal static readonly int[] BoundsIds = {
            Shader.PropertyToID("_MotuSquall"), Shader.PropertyToID("_MotuSquall1"),
            Shader.PropertyToID("_MotuSquall2") };
        internal static readonly int[] WindIds = {
            Shader.PropertyToID("_MotuSquallWind"), Shader.PropertyToID("_MotuSquallWind1"),
            Shader.PropertyToID("_MotuSquallWind2") };

        internal static void Publish(int slot, WorldSquallField field)
        {
            Shader.SetGlobalVector(BoundsIds[slot], field.ShaderBounds);
            Shader.SetGlobalVector(WindIds[slot], field.ShaderWind);
        }

        internal static void SnapshotTo(Material material)
        {
            for (var slot = 0; slot < BoundsIds.Length; slot++)
            {
                material.SetVector(BoundsIds[slot], Shader.GetGlobalVector(BoundsIds[slot]));
                material.SetVector(WindIds[slot], Shader.GetGlobalVector(WindIds[slot]));
            }
        }
    }
}
