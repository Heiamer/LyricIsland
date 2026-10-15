namespace LyricIsland;
public static class IslandMaterials
{
    public static bool IsValid(string? value) => value is "Default";
    // Older configurations migrate to the single supported material on load.
    public static string Normalize(string? value) => "Default";
}
