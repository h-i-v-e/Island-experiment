using System;
using System.IO;
using System.Security.Cryptography;
using UnityEngine;

namespace Motu.Islands
{
    // Small, independently readable sidecar; never deserializes the native island.
    internal static class IslandHorizonCache
    {
        internal const string Extension = ".motuhorizon";
        private const int Magic = 0x4D484F52;
        private const int Version = 1;
        private const int MaximumBytes = 32 * 1024 * 1024;

        internal static string PathFor(string snapshotPath) => string.IsNullOrEmpty(snapshotPath)
            ? null : Path.ChangeExtension(snapshotPath, Extension);

        internal static bool TryLoad(string snapshotPath, out IslandPreparedMesh mesh)
        {
            mesh = null;
            var path = PathFor(snapshotPath);
            if (path == null || !File.Exists(path)) return false;
            try
            {
                var length = new FileInfo(path).Length;
                if (length < 48 || length > MaximumBytes) return false;
                var bytes = File.ReadAllBytes(path);
                if (bytes.Length < 48 || bytes.Length > MaximumBytes) return false;
                using var sha = SHA256.Create();
                var hash = sha.ComputeHash(bytes, 0, bytes.Length - 32);
                for (var i = 0; i < hash.Length; i++)
                    if (hash[i] != bytes[bytes.Length - 32 + i]) return false;
                using var stream = new MemoryStream(bytes, 0, bytes.Length - 32, false);
                using var reader = new BinaryReader(stream);
                if (reader.ReadInt32() != Magic || reader.ReadInt32() != Version) return false;
                var vertexCount = reader.ReadInt32();
                var indexCount = reader.ReadInt32();
                if (vertexCount < 0 || indexCount < 0 || indexCount % 3 != 0
                    || (vertexCount == 0) != (indexCount == 0)
                    || 16L + vertexCount * 12L + indexCount * 4L != stream.Length) return false;
                var vertices = new Vector3[vertexCount];
                var indices = new int[indexCount];
                for (var i = 0; i < vertices.Length; i++)
                {
                    var vertex = new Vector3(reader.ReadSingle(), reader.ReadSingle(), reader.ReadSingle());
                    if (!Finite(vertex.x) || !Finite(vertex.y) || !Finite(vertex.z)) return false;
                    vertices[i] = vertex;
                }
                for (var i = 0; i < indices.Length; i++)
                {
                    var index = reader.ReadInt32();
                    if (index < 0 || index >= vertexCount) return false;
                    indices[i] = index;
                }
                if (vertexCount > 0)
                    mesh = new IslandPreparedMesh(vertices, Array.Empty<Vector3>(), indices,
                        Array.Empty<Vector2>(), Array.Empty<Color>(), Array.Empty<Vector2>());
                try { File.SetLastWriteTimeUtc(path, DateTime.UtcNow); }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
                return true; // A null mesh is a valid fully submerged island.
            }
            catch (IOException) { return false; }
            catch (UnauthorizedAccessException) { return false; }
        }

        internal static bool TrySave(string snapshotPath, IslandPreparedMesh mesh, long budgetBytes)
        {
            var path = PathFor(snapshotPath);
            if (path == null) return false;
            var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                using var stream = new MemoryStream();
                using (var writer = new BinaryWriter(stream, System.Text.Encoding.UTF8, true))
                {
                    writer.Write(Magic); writer.Write(Version);
                    writer.Write(mesh?.vertices.Length ?? 0); writer.Write(mesh?.triangles.Length ?? 0);
                    if (mesh != null)
                    {
                        foreach (var vertex in mesh.vertices)
                        { writer.Write(vertex.x); writer.Write(vertex.y); writer.Write(vertex.z); }
                        foreach (var index in mesh.triangles) writer.Write(index);
                    }
                }
                if (stream.Length + 32 > MaximumBytes) return false;
                var bytes = stream.ToArray();
                using var sha = SHA256.Create();
                var hash = sha.ComputeHash(bytes);
                Directory.CreateDirectory(Path.GetDirectoryName(path));
                using (var file = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write))
                { file.Write(bytes, 0, bytes.Length); file.Write(hash, 0, hash.Length); }
                if (File.Exists(path)) File.Replace(temporary, path, null);
                else File.Move(temporary, path);
                IslandSnapshotCache.TrimToBudget(Path.GetDirectoryName(path), budgetBytes, snapshotPath);
                return true;
            }
            catch (IOException error) { Debug.LogWarning($"Could not cache island horizon: {error.Message}"); return false; }
            catch (UnauthorizedAccessException error) { Debug.LogWarning($"Could not cache island horizon: {error.Message}"); return false; }
            finally
            {
                try { if (File.Exists(temporary)) File.Delete(temporary); }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
        }

        private static bool Finite(float value) => !float.IsNaN(value) && !float.IsInfinity(value);
    }
}
