using System.IO;

namespace LyricIsland.Media;

// Scan a PE section without copying the whole code section into the managed heap.
internal static class PlayerSignatureScanner
{
    internal const int BlockSize = 64 * 1024;
    public static nint Resolve(Stream file, long sectionOffset, int sectionLength, nint sectionAddress, string signature, int displacementOffset)
    {
        var pattern = signature.Split(' ').Select(s => s == "?" ? -1 : Convert.ToInt32(s, 16)).ToArray();
        if (pattern.Length < displacementOffset + 4 || pattern.Length > BlockSize || sectionLength < pattern.Length) throw new IOException("Invalid player signature section.");
        var buffer = new byte[BlockSize + pattern.Length - 1];
        var overlap = pattern.Length - 1;
        var carry = 0;
        var consumed = 0;
        var found = -1;
        var displacement = 0;
        file.Position = sectionOffset;
        while (consumed < sectionLength)
        {
            var requested = Math.Min(BlockSize, sectionLength - consumed);
            var count = 0;
            while (count < requested)
            {
                var read = file.Read(buffer, carry + count, requested - count);
                if (read == 0) throw new EndOfStreamException("Player code section was truncated.");
                count += read;
            }
            var total = carry + count;
            var firstOffset = consumed - carry;
            for (var i = 0; i <= total - pattern.Length; i++)
            {
                if (buffer[i] != pattern[0]) continue;
                var matches = true;
                for (var j = 1; j < pattern.Length; j++) if (pattern[j] >= 0 && buffer[i + j] != pattern[j]) { matches = false; break; }
                if (!matches) continue;
                if (found >= 0) throw new IOException("Ambiguous player signature.");
                found = firstOffset + i;
                displacement = BitConverter.ToInt32(buffer, i + displacementOffset);
            }
            consumed += count;
            carry = Math.Min(overlap, total);
            Buffer.BlockCopy(buffer, total - carry, buffer, 0, carry);
        }
        if (found < 0) throw new IOException("Unsupported player build.");
        return sectionAddress + found + displacementOffset + 4 + displacement;
    }
}
