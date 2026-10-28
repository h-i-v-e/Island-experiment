using UnityEngine;

namespace Motu.Rendering
{
    /// <summary>
    /// Immutable, periodic cloud volumes shared by all environments. World weather
    /// remains seed-specific; these small volumes describe only local cloud shape.
    /// </summary>
    internal static class CloudNoiseTextures
    {
        private static readonly int ShapeId = Shader.PropertyToID("_MotuCloudShapeTex");
        private static readonly int ErosionId = Shader.PropertyToID("_MotuCloudErosionTex");
        private static Texture3D shape;
        private static Texture3D erosion;

        static CloudNoiseTextures()
        {
            Application.quitting += Release;
#if UNITY_EDITOR
            UnityEditor.AssemblyReloadEvents.beforeAssemblyReload += Release;
#endif
        }

        // Called on initialization/resume, never from the per-frame render path.
        internal static void EnsureBound()
        {
            if (shape == null) shape = CreateShape();
            if (erosion == null) erosion = CreateErosion();
            Shader.SetGlobalTexture(ShapeId, shape);
            Shader.SetGlobalTexture(ErosionId, erosion);
        }

        private static Texture3D CreateShape()
        {
            const int size = 64;
            var smooth0 = new PeriodicLattice(4, 0x17a3c91du);
            var smooth1 = new PeriodicLattice(8, 0xe3d972a1u);
            var smooth2 = new PeriodicLattice(16, 0x5d213b7fu);
            var cells0 = new PeriodicLattice(3, 0x72491ca3u);
            var cells1 = new PeriodicLattice(6, 0x182f6d97u);
            var cells2 = new PeriodicLattice(12, 0xa24d765bu);
            var pixels = new Color32[size * size * size];
            for (var z = 0; z < size; z++)
            for (var y = 0; y < size; y++)
            for (var x = 0; x < size; x++)
            {
                var p = new Vector3((x + 0.5f) / size, (y + 0.5f) / size, (z + 0.5f) / size);
                var connected = smooth0.Value(p) * 0.625f + smooth1.Value(p) * 0.25f + smooth2.Value(p) * 0.125f;
                var billows = cells0.Worley(p) * 0.65f + cells1.Worley(p) * 0.25f + cells2.Worley(p) * 0.10f;
                pixels[x + size * (y + size * z)] = new Color32(
                    Byte((connected - 0.5f) * 1.45f + 0.5f), Byte(billows), Byte(smooth2.Value(p)), 255);
            }
            return Upload("Cloud connected shape and billows", size, pixels);
        }

        private static Texture3D CreateErosion()
        {
            const int size = 32;
            var cells0 = new PeriodicLattice(4, 0x0a73c49bu);
            var cells1 = new PeriodicLattice(8, 0x937e21d5u);
            var cells2 = new PeriodicLattice(12, 0x2879b65du);
            var pixels = new Color32[size * size * size];
            for (var z = 0; z < size; z++)
            for (var y = 0; y < size; y++)
            for (var x = 0; x < size; x++)
            {
                var p = new Vector3((x + 0.5f) / size, (y + 0.5f) / size, (z + 0.5f) / size);
                var coarse = cells0.Worley(p);
                var medium = cells1.Worley(p);
                var fine = cells2.Worley(p);
                pixels[x + size * (y + size * z)] = new Color32(
                    Byte(coarse * 0.625f + medium * 0.25f + fine * 0.125f), Byte(medium), Byte(fine), 255);
            }
            return Upload("Cloud fine erosion", size, pixels);
        }

        private static Texture3D Upload(string name, int size, Color32[] pixels)
        {
            var texture = new Texture3D(size, size, size, TextureFormat.RGBA32, true)
            {
                name = name,
                hideFlags = HideFlags.HideAndDontSave,
                wrapMode = TextureWrapMode.Repeat,
                filterMode = FilterMode.Trilinear,
                anisoLevel = 0,
            };
            texture.SetPixels32(pixels);
            texture.Apply(true, true);
            return texture;
        }

        private static byte Byte(float value) => (byte)Mathf.RoundToInt(Mathf.Clamp01(value) * 255f);

        private static void Release()
        {
            UnityObjectLifetime.DestroyUnityObject(shape);
            UnityObjectLifetime.DestroyUnityObject(erosion);
            shape = null;
            erosion = null;
        }

        private sealed class PeriodicLattice
        {
            private readonly int period;
            private readonly float[] values;
            private readonly Vector3[] features;

            internal PeriodicLattice(int period, uint seed)
            {
                this.period = period;
                values = new float[period * period * period];
                features = new Vector3[values.Length];
                for (var z = 0; z < period; z++)
                for (var y = 0; y < period; y++)
                for (var x = 0; x < period; x++)
                {
                    var index = Index(x, y, z);
                    var hash = Hash(seed ^ (uint)x * 0x9e3779b9u ^ (uint)y * 0x85ebca6bu ^ (uint)z * 0xc2b2ae35u);
                    values[index] = Unit(hash);
                    // Keep feature points off the cell edge; neighboring cells
                    // still determine each sample, including across texture wraps.
                    features[index] = new Vector3(
                        0.15f + Unit(Hash(hash ^ 0x68bc21ebu)) * 0.7f,
                        0.15f + Unit(Hash(hash ^ 0x02e5be93u)) * 0.7f,
                        0.15f + Unit(Hash(hash ^ 0x967a889bu)) * 0.7f);
                }
            }

            internal float Value(Vector3 unitPosition)
            {
                var p = unitPosition * period;
                var x = Mathf.FloorToInt(p.x);
                var y = Mathf.FloorToInt(p.y);
                var z = Mathf.FloorToInt(p.z);
                var fx = Fade(p.x - x);
                var fy = Fade(p.y - y);
                var fz = Fade(p.z - z);
                var near = Mathf.Lerp(
                    Mathf.Lerp(At(x, y, z), At(x + 1, y, z), fx),
                    Mathf.Lerp(At(x, y + 1, z), At(x + 1, y + 1, z), fx), fy);
                var far = Mathf.Lerp(
                    Mathf.Lerp(At(x, y, z + 1), At(x + 1, y, z + 1), fx),
                    Mathf.Lerp(At(x, y + 1, z + 1), At(x + 1, y + 1, z + 1), fx), fy);
                return Mathf.Lerp(near, far, fz);
            }

            internal float Worley(Vector3 unitPosition)
            {
                var p = unitPosition * period;
                var x = Mathf.FloorToInt(p.x);
                var y = Mathf.FloorToInt(p.y);
                var z = Mathf.FloorToInt(p.z);
                var nearestSquared = 3f;
                for (var dz = -1; dz <= 1; dz++)
                for (var dy = -1; dy <= 1; dy++)
                for (var dx = -1; dx <= 1; dx++)
                {
                    var nx = x + dx;
                    var ny = y + dy;
                    var nz = z + dz;
                    var feature = features[Index(Wrap(nx), Wrap(ny), Wrap(nz))];
                    var deltaX = nx + feature.x - p.x;
                    var deltaY = ny + feature.y - p.y;
                    var deltaZ = nz + feature.z - p.z;
                    var squared = deltaX * deltaX + deltaY * deltaY + deltaZ * deltaZ;
                    if (squared < nearestSquared) nearestSquared = squared;
                }
                return Mathf.Clamp01(1f - Mathf.Sqrt(nearestSquared) * 1.1f);
            }

            private float At(int x, int y, int z) => values[Index(Wrap(x), Wrap(y), Wrap(z))];
            private int Index(int x, int y, int z) => x + period * (y + period * z);
            private int Wrap(int coordinate) => (coordinate + period) % period;
            private static float Fade(float t) => t * t * t * (t * (t * 6f - 15f) + 10f);
            private static float Unit(uint value) => (value & 0x00ffffffu) / 16777215f;
            private static uint Hash(uint value)
            {
                unchecked
                {
                    value ^= value >> 16;
                    value *= 0x7feb352du;
                    value ^= value >> 15;
                    value *= 0x846ca68bu;
                    return value ^ (value >> 16);
                }
            }
        }
    }
}
