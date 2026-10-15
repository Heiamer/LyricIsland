using System.IO;
using System.IO.Pipes;
using System.Text.Json;

namespace LyricIsland;

internal sealed class SettingsServer : IDisposable
{
    private readonly CancellationTokenSource _lifetime = new();
    private readonly Func<JsonElement, Task<object>> _handle;
    private readonly Task _server;
    public SettingsServer(Func<JsonElement, Task<object>> handler) { _handle = handler; _server = RunAsync(); }
    private async Task RunAsync()
    {
        while (!_lifetime.IsCancellationRequested)
        {
            try
            {
                await using var pipe = new NamedPipeServerStream(SettingsProtocol.PipeName, PipeDirection.InOut, 1,
                    PipeTransmissionMode.Byte, PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
                await pipe.WaitForConnectionAsync(_lifetime.Token);
                using var requestTimeout = CancellationTokenSource.CreateLinkedTokenSource(_lifetime.Token);
                requestTimeout.CancelAfter(TimeSpan.FromSeconds(8));
                using var reader = new StreamReader(pipe, leaveOpen: true);
                using var writer = new StreamWriter(pipe, leaveOpen: true) { AutoFlush = true };
                var line = await reader.ReadLineAsync(requestTimeout.Token);
                object response;
                if (line is null || line.Length > 16384) response = new { success = false, message = "请求无效。" };
                else
                {
                    using var document = JsonDocument.Parse(line);
                    response = await _handle(document.RootElement.Clone());
                }
                await writer.WriteLineAsync(JsonSerializer.Serialize(response));
            }
            catch (OperationCanceledException) when (_lifetime.IsCancellationRequested) { return; }
            catch (Exception ex) when (ex is IOException or JsonException or OperationCanceledException)
            {
                TaskbarLyrics.Core.Utilities.Log.Warn($"Settings connection: {ex.Message}");
                try { await Task.Delay(150, _lifetime.Token); }
                catch (OperationCanceledException) when (_lifetime.IsCancellationRequested) { return; }
            }
        }
    }
    public void Dispose()
    {
        _lifetime.Cancel();
        _ = _server.ContinueWith(task => { if (task.IsFaulted) TaskbarLyrics.Core.Utilities.Log.Warn(task.Exception!.Message); _lifetime.Dispose(); }, TaskScheduler.Default);
    }
}
