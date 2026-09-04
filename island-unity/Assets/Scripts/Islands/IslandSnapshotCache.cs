using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using UnityEngine;

internal static class IslandSnapshotCache
{
    // Generation semantics changed: submerged river floors now use the normal
    // channel depth below sea level and preserve already-deeper terrain.
    private const int CacheKeySchemaVersion = 4;
    private const string SnapshotExtension = ".motusnapshot";

    internal static string CacheDirectory
    {
        get
        {
            var directory = System.IO.Path.Combine(
                Application.persistentDataPath,
                "GeneratedIslandCache");
            Directory.CreateDirectory(directory);
            return directory;
        }
    }

    internal static string PathFor(IslandGenerationRequest request)
    {
        if (request == null) throw new ArgumentNullException(nameof(request));
        var directory = CacheDirectory;
        return System.IO.Path.Combine(directory, HashRequest(request) + SnapshotExtension);
    }

    internal static IntPtr TryLoad(string path, out int status)
    {
        if (string.IsNullOrEmpty(path) || !File.Exists(path))
        {
            status = 2;
            return IntPtr.Zero;
        }
        var handle = MotuNative.LoadMotuSnapshot(path, out status);
        if (handle != IntPtr.Zero)
        {
            File.SetLastWriteTimeUtc(path, DateTime.UtcNow);
            return handle;
        }
        if (status == 3)
        {
            TryDelete(path);
        }
        return IntPtr.Zero;
    }

    internal static bool TrySave(IntPtr handle, string path, long budgetBytes)
    {
        if (handle == IntPtr.Zero || string.IsNullOrEmpty(path))
        {
            return false;
        }
        var status = MotuNative.SaveMotuSnapshot(handle, path);
        if (status != 0)
        {
            return false;
        }
        TrimToBudget(System.IO.Path.GetDirectoryName(path), budgetBytes, path);
        return true;
    }

    private static string HashRequest(IslandGenerationRequest request)
    {
        using var bytes = new MemoryStream();
        using (var writer = new BinaryWriter(bytes, Encoding.UTF8, true))
        {
            writer.Write(CacheKeySchemaVersion);
            writer.Write(request.Descriptor.GeneratorSchemaVersion);
            writer.Write(request.Descriptor.Seed);
            writer.Write(request.WorldSizeMetres);
            Write(writer, request.Options);
            Write(writer, request.ForestOptions);
            Write(writer, request.ReedOptions);
            Write(writer, request.FernOptions);
        }
        using var sha = SHA256.Create();
        return BitConverter.ToString(sha.ComputeHash(bytes.ToArray()))
            .Replace("-", string.Empty)
            .ToLowerInvariant();
    }

    private static void Write(BinaryWriter writer, MotuNative.Options value)
    {
        writer.Write(value.maxZ);
        writer.Write(value.waterRatio);
        writer.Write(value.slopeMultiplier);
        writer.Write(value.coastalSlopeMultiplier);
        writer.Write(value.continentalNoiseFrequency);
        writer.Write(value.detailNoiseFrequency);
        writer.Write(value.hydraulicErosionStrength);
        writer.Write(value.hydraulicDepositionStrength);
        writer.Write(value.hydraulicDepositionSlopeDegrees);
        writer.Write(value.riverSourceCatchmentHectares);
        writer.Write(value.riverSourceSteepMultiplier);
        writer.Write(value.riverSourceElevationBoost);
        writer.Write(value.riverSourceWidthMetres);
        writer.Write(value.riverMaximumWidthMetres);
        writer.Write(value.riverSourceDepthMetres);
        writer.Write(value.riverMaximumDepthMetres);
        writer.Write(value.continentalNoiseStrength);
        writer.Write(value.detailNoiseStrength);
        writer.Write(value.landMassOffset);
    }

    private static void Write(BinaryWriter writer, MotuNative.ForestOptions value)
    {
        writer.Write(value.patchSizeMetres);
        writer.Write(value.noiseThreshold);
        writer.Write(value.noiseOctaves);
        writer.Write(value.snowlineMetres);
        writer.Write(value.prototypeCount);
        writer.Write(value.minimumScale);
        writer.Write(value.maximumScale);
    }

    private static void Write(BinaryWriter writer, MotuNative.ReedOptions value)
    {
        writer.Write(value.bankWidthMetres);
        writer.Write(value.patchSizeMetres);
        writer.Write(value.coverageThreshold);
        writer.Write(value.spacingMetres);
        writer.Write(value.rushRatio);
        writer.Write(value.minimumHeightMetres);
        writer.Write(value.maximumHeightMetres);
        writer.Write(value.maximumSlopeDegrees);
    }

    private static void Write(BinaryWriter writer, MotuNative.FernOptions value)
    {
        writer.Write(value.barkClearanceMetres);
        writer.Write(value.outerRadiusMetres);
        writer.Write(value.spacingMetres);
        writer.Write(value.patchSizeMetres);
        writer.Write(value.coverageThreshold);
        writer.Write(value.minimumLengthMetres);
        writer.Write(value.maximumLengthMetres);
        writer.Write(value.maximumSlopeDegrees);
    }

    internal static void TrimToBudget(
        string directory,
        long budgetBytes,
        string protectedPath)
    {
        if (string.IsNullOrEmpty(directory) || budgetBytes <= 0 || !Directory.Exists(directory))
        {
            return;
        }
        try
        {
            var files = new DirectoryInfo(directory).GetFiles("*.motu*");
            Array.Sort(files, (left, right) =>
                left.LastWriteTimeUtc.CompareTo(right.LastWriteTimeUtc));
            long total = 0;
            foreach (var file in files)
            {
                total = checked(total + file.Length);
            }
            foreach (var file in files)
            {
                if (total <= budgetBytes)
                {
                    break;
                }
                if (string.Equals(file.FullName, protectedPath, StringComparison.Ordinal))
                {
                    continue;
                }
                var length = file.Length;
                file.Delete();
                total -= length;
            }
        }
        catch (Exception error)
        {
            Debug.LogWarning($"Could not trim the generated-island cache: {error.Message}");
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch (Exception error)
        {
            Debug.LogWarning($"Could not remove invalid island snapshot '{path}': {error.Message}");
        }
    }
}
