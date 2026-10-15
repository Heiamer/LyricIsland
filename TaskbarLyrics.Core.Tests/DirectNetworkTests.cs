using Xunit;
namespace TaskbarLyrics.Core.Tests;
public sealed class DirectNetworkTests
{
    [Fact]
    public void HandlerExplicitlyDisablesAllInheritedProxyRoutes()
    {
        using var handler = DirectNetwork.CreateHandler();
        Assert.False(handler.UseProxy); Assert.Null(handler.Proxy);
        using var client = DirectNetwork.CreateClient(TimeSpan.FromSeconds(5));
        Assert.Equal(TimeSpan.FromSeconds(5), client.Timeout);
    }
}
