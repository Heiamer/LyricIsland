using LyricIsland.Media;
using System.IO;
using Xunit;

namespace LyricIsland.Tests;

public sealed class PlayerSignatureScannerTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(65530)]
    [InlineData(65533)]
    [InlineData(131071)]
    public void ResolvesDisplacementAcrossBlockEdges(int offset)
    {
        var bytes = new byte[200000];
        bytes[offset] = 0x48; bytes[offset + 1] = 0x8D; bytes[offset + 2] = 0x0D;
        BitConverter.GetBytes(-20).CopyTo(bytes, offset + 3); bytes[offset + 7] = 0xE8;
        using var file = new MemoryStream(bytes);
        Assert.Equal((nint)(1000 + offset + 3 + 4 - 20), PlayerSignatureScanner.Resolve(file, 0, bytes.Length, 1000, "48 8D 0D ? ? ? ? E8", 3));
    }
    [Fact]
    public void DoesNotSilentlyAcceptMultipleMatchesOrTruncatedSection()
    {
        var bytes = new byte[100]; bytes[5] = bytes[50] = 0x48;
        using var file = new MemoryStream(bytes);
        Assert.Throws<IOException>(() => PlayerSignatureScanner.Resolve(file, 0, bytes.Length, 0, "48 ? ? ? ?", 1));
        Assert.Throws<EndOfStreamException>(() => PlayerSignatureScanner.Resolve(file, 0, bytes.Length + 1, 0, "49 ? ? ? ?", 1));
    }
    [Fact]
    public void ScanningLargeSectionDoesNotAllocateTheWholeSection()
    {
        var bytes = new byte[16 * 1024 * 1024]; bytes[bytes.Length - 8] = 0x48;
        using var file = new MemoryStream(bytes);
        var before = GC.GetAllocatedBytesForCurrentThread();
        PlayerSignatureScanner.Resolve(file, 0, bytes.Length, 0, "48 ? ? ? ?", 1);
        Assert.InRange(GC.GetAllocatedBytesForCurrentThread() - before, 1, 128 * 1024);
    }
}
