using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Media.Animation;

namespace LyricIsland;

internal static class AnimationCadence
{
    public static int TargetRate { get; private set; } = 165;
    public static void Initialize()
    {
        try
        {
            // Read the active display mode; DEVMODEW.dmDisplayFrequency is at offset 184.
            var mode = Marshal.AllocHGlobal(220);
            try
            {
                for (var i = 0; i < 220; i++) Marshal.WriteByte(mode, i, 0);
                Marshal.WriteInt16(mode, 68, 220);
                if (EnumDisplaySettings(null, -1, mode))
                {
                    var hz = Marshal.ReadInt32(mode, 184);
                    if (hz >= 30 && hz <= 500) TargetRate = hz;
                }
            }
            finally { Marshal.FreeHGlobal(mode); }
            Timeline.DesiredFrameRateProperty.OverrideMetadata(typeof(Timeline), new FrameworkPropertyMetadata((int?)TargetRate));
        }
        catch (InvalidOperationException) { }
    }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool EnumDisplaySettings(string? device, int mode, IntPtr settings);
}
