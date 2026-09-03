using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using UnityEngine;

internal static class IslandMaterialTextureCache
{
    private const int FormatVersion = 1;
    private const int ChecksumBytes = 32;
    private const string Extension = ".motumaterials";
    private static readonly byte[] Magic = Encoding.ASCII.GetBytes("MOTUMAT1");

    internal static IslandPreparedMaterialTextures TryLoad(
        IslandMaterialColours colours,
        int resolution,
        long cacheBudgetBytes,
        string cacheDirectory)
    {
        if (cacheBudgetBytes <= 0 || string.IsNullOrEmpty(cacheDirectory))
        {
            return null;
        }
        var revision = MotuNative.GetMotuRuntimeMaterialRevision();
        var path = PathFor(colours, resolution, revision, cacheDirectory);
        if (!File.Exists(path))
        {
            return null;
        }
        try
        {
            var result = Read(path, colours, resolution, revision);
            File.SetLastWriteTimeUtc(path, DateTime.UtcNow);
            return result;
        }
        catch (InvalidDataException)
        {
            TryDelete(path);
            return null;
        }
        catch (IOException)
        {
            return null;
        }
        catch (UnauthorizedAccessException)
        {
            return null;
        }
    }

    internal static void TrySave(
        IslandPreparedMaterialTextures textures,
        long cacheBudgetBytes,
        string cacheDirectory)
    {
        if (textures == null
            || cacheBudgetBytes <= 0
            || string.IsNullOrEmpty(cacheDirectory))
        {
            return;
        }
        var revision = MotuNative.GetMotuRuntimeMaterialRevision();
        var resolution = textures.dirt.width;
        var path = PathFor(textures.colours, resolution, revision, cacheDirectory);
        if (File.Exists(path))
        {
            File.SetLastWriteTimeUtc(path, DateTime.UtcNow);
            return;
        }
        var temporaryPath = path + ".tmp-" + Guid.NewGuid().ToString("N");
        try
        {
            Write(temporaryPath, textures, revision);
            try
            {
                File.Move(temporaryPath, path);
            }
            catch (IOException) when (File.Exists(path))
            {
                TryDelete(temporaryPath);
            }
            IslandSnapshotCache.TrimToBudget(
                Path.GetDirectoryName(path),
                cacheBudgetBytes,
                path);
        }
        catch (IOException)
        {
            TryDelete(temporaryPath);
        }
        catch (UnauthorizedAccessException)
        {
            TryDelete(temporaryPath);
        }
    }

    internal static void ValidateRoundTrip()
    {
        var directory = Path.Combine(
            Path.GetTempPath(),
            "motu-material-cache-validation-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            const int resolution = 2;
            var colours = new IslandMaterialColours(
                new Color(0.11f, 0.07f, 0.03f),
                new Color(0.24f, 0.26f, 0.28f),
                new Color(0.62f, 0.48f, 0.21f));
            var texture = new IslandPreparedMaterialTexture(
                resolution,
                resolution,
                -0.01f,
                0.03f,
                0f,
                Sequence(resolution * resolution * 3, 7),
                Sequence(resolution * resolution * 3, 23),
                Sequence(resolution * resolution * 2, 41),
                Sequence(resolution * resolution, 59));
            var expected = new IslandPreparedMaterialTextures(
                colours,
                texture,
                texture,
                texture,
                texture,
                texture,
                texture);
            TrySave(expected, 1024L * 1024L, directory);
            var actual = TryLoad(colours, resolution, 1024L * 1024L, directory);
            if (actual == null
                || !actual.loadedFromCache
                || !Same(texture, actual.dirt)
                || !Same(texture, actual.forestFloor)
                || !Same(texture, actual.rock)
                || !Same(texture, actual.riverBed)
                || !Same(texture, actual.beach)
                || !Same(texture, actual.fallenStones))
            {
                throw new InvalidOperationException(
                    "The content-addressed material cache did not round-trip its maps.");
            }
        }
        finally
        {
            Directory.Delete(directory, true);
        }
    }

    private static IslandPreparedMaterialTextures Read(
        string path,
        IslandMaterialColours colours,
        int resolution,
        MotuNative.MaterialRevision expectedRevision)
    {
        var fileLength = new FileInfo(path).Length;
        if (fileLength <= ChecksumBytes)
        {
            throw new InvalidDataException("Material cache file is truncated.");
        }
        var payloadLength = fileLength - ChecksumBytes;
        var actualChecksum = HashPrefix(path, payloadLength);
        var expectedChecksum = new byte[ChecksumBytes];
        using (var checksumSource = File.OpenRead(path))
        {
            checksumSource.Position = payloadLength;
            ReadExactly(checksumSource, expectedChecksum);
        }
        if (!Equal(actualChecksum, expectedChecksum))
        {
            throw new InvalidDataException("Material cache checksum mismatch.");
        }

        using var source = File.OpenRead(path);
        using var reader = new BinaryReader(source, Encoding.UTF8, true);
        if (!Equal(reader.ReadBytes(Magic.Length), Magic)
            || reader.ReadInt32() != FormatVersion
            || reader.ReadUInt64() != expectedRevision.low
            || reader.ReadUInt64() != expectedRevision.high
            || reader.ReadInt32() != resolution)
        {
            throw new InvalidDataException("Material cache header is obsolete or invalid.");
        }
        var result = new IslandPreparedMaterialTextures(
            colours,
            ReadTexture(reader, resolution),
            ReadTexture(reader, resolution),
            ReadTexture(reader, resolution),
            ReadTexture(reader, resolution),
            ReadTexture(reader, resolution),
            ReadTexture(reader, resolution),
            true);
        if (source.Position != payloadLength)
        {
            throw new InvalidDataException("Material cache payload has an invalid length.");
        }
        return result;
    }

    private static void Write(
        string path,
        IslandPreparedMaterialTextures textures,
        MotuNative.MaterialRevision revision)
    {
        using (var destination = new FileStream(
            path,
            FileMode.CreateNew,
            FileAccess.Write,
            FileShare.None))
        using (var writer = new BinaryWriter(destination, Encoding.UTF8, true))
        {
            writer.Write(Magic);
            writer.Write(FormatVersion);
            writer.Write(revision.low);
            writer.Write(revision.high);
            writer.Write(textures.dirt.width);
            WriteTexture(writer, textures.dirt);
            WriteTexture(writer, textures.forestFloor);
            WriteTexture(writer, textures.rock);
            WriteTexture(writer, textures.riverBed);
            WriteTexture(writer, textures.beach);
            WriteTexture(writer, textures.fallenStones);
            writer.Flush();
            destination.Flush(true);
        }
        var checksum = HashPrefix(path, new FileInfo(path).Length);
        using var append = new FileStream(path, FileMode.Append, FileAccess.Write, FileShare.None);
        append.Write(checksum, 0, checksum.Length);
        append.Flush(true);
    }

    private static IslandPreparedMaterialTexture ReadTexture(
        BinaryReader reader,
        int resolution)
    {
        var pixels = checked(resolution * resolution);
        var minimumHeight = reader.ReadSingle();
        var maximumHeight = reader.ReadSingle();
        var baseHeight = reader.ReadSingle();
        var albedo = ReadBytes(reader, checked(pixels * 3));
        var normal = ReadBytes(reader, checked(pixels * 3));
        var height = ReadBytes(reader, checked(pixels * 2));
        var occlusion = ReadBytes(reader, pixels);
        return new IslandPreparedMaterialTexture(
            resolution,
            resolution,
            minimumHeight,
            maximumHeight,
            baseHeight,
            albedo,
            normal,
            height,
            occlusion);
    }

    private static void WriteTexture(
        BinaryWriter writer,
        IslandPreparedMaterialTexture texture)
    {
        writer.Write(texture.minimumHeight);
        writer.Write(texture.maximumHeight);
        writer.Write(texture.baseHeight);
        writer.Write(texture.albedoRgb);
        writer.Write(texture.normalRgb);
        writer.Write(texture.heightR16);
        writer.Write(texture.occlusion);
    }

    private static string PathFor(
        IslandMaterialColours colours,
        int resolution,
        MotuNative.MaterialRevision revision,
        string directory)
    {
        using var key = new MemoryStream();
        using (var writer = new BinaryWriter(key, Encoding.UTF8, true))
        {
            writer.Write(FormatVersion);
            writer.Write(revision.low);
            writer.Write(revision.high);
            writer.Write(resolution);
            var inputs = colours.ToNative();
            writer.Write(inputs.dirtRed);
            writer.Write(inputs.dirtGreen);
            writer.Write(inputs.dirtBlue);
            writer.Write(inputs.stoneRed);
            writer.Write(inputs.stoneGreen);
            writer.Write(inputs.stoneBlue);
            writer.Write(inputs.sandRed);
            writer.Write(inputs.sandGreen);
            writer.Write(inputs.sandBlue);
        }
        using var sha = SHA256.Create();
        var hash = BitConverter.ToString(sha.ComputeHash(key.ToArray()))
            .Replace("-", string.Empty)
            .ToLowerInvariant();
        return Path.Combine(directory, hash + Extension);
    }

    private static byte[] HashPrefix(string path, long length)
    {
        using var source = File.OpenRead(path);
        using var sha = SHA256.Create();
        var buffer = new byte[64 * 1024];
        var remaining = length;
        while (remaining > 0)
        {
            var count = source.Read(
                buffer,
                0,
                (int)Math.Min(buffer.Length, remaining));
            if (count <= 0)
            {
                throw new InvalidDataException("Material cache file is truncated.");
            }
            sha.TransformBlock(buffer, 0, count, buffer, 0);
            remaining -= count;
        }
        sha.TransformFinalBlock(Array.Empty<byte>(), 0, 0);
        return sha.Hash;
    }

    private static byte[] ReadBytes(BinaryReader reader, int length)
    {
        var result = reader.ReadBytes(length);
        if (result.Length != length)
        {
            throw new InvalidDataException("Material cache texture data is truncated.");
        }
        return result;
    }

    private static void ReadExactly(Stream source, byte[] destination)
    {
        var offset = 0;
        while (offset < destination.Length)
        {
            var count = source.Read(destination, offset, destination.Length - offset);
            if (count <= 0)
            {
                throw new InvalidDataException("Material cache checksum is truncated.");
            }
            offset += count;
        }
    }

    private static bool Equal(byte[] left, byte[] right)
    {
        if (left == null || right == null || left.Length != right.Length)
        {
            return false;
        }
        var difference = 0;
        for (var index = 0; index < left.Length; index++)
        {
            difference |= left[index] ^ right[index];
        }
        return difference == 0;
    }

    private static void TryDelete(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch
        {
        }
    }

    private static byte[] Sequence(int length, int offset)
    {
        var result = new byte[length];
        for (var index = 0; index < length; index++)
        {
            result[index] = (byte)(index + offset);
        }
        return result;
    }

    private static bool Same(
        IslandPreparedMaterialTexture expected,
        IslandPreparedMaterialTexture actual)
    {
        return actual != null
            && expected.width == actual.width
            && expected.height == actual.height
            && expected.minimumHeight.Equals(actual.minimumHeight)
            && expected.maximumHeight.Equals(actual.maximumHeight)
            && expected.baseHeight.Equals(actual.baseHeight)
            && Equal(expected.albedoRgb, actual.albedoRgb)
            && Equal(expected.normalRgb, actual.normalRgb)
            && Equal(expected.heightR16, actual.heightR16)
            && Equal(expected.occlusion, actual.occlusion);
    }
}
