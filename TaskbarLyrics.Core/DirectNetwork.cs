using System.Net.Http;

namespace TaskbarLyrics.Core;

// Requests are always direct. Never inherit Windows, PAC or environment proxies.
// This configures application clients only; it does not change system settings.
public static class DirectNetwork
{
    internal static HttpClientHandler CreateHandler() => new() { UseProxy = false, Proxy = null };
    public static HttpClient CreateClient(TimeSpan timeout) => new(CreateHandler()) { Timeout = timeout };
}
