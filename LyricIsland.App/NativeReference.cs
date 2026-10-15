using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
namespace LyricIsland;
internal static class NativeReference
{
    internal static void Benchmark()
    {
        var window=new MainWindow(new IslandSettings{Animation=true,AlwaysOnTop=false},demo:true);
        window.Show();window.SetDemo(true);
        var timer=new System.Windows.Threading.DispatcherTimer{Interval=TimeSpan.FromSeconds(12)};
        timer.Tick+=(_,_)=>{timer.Stop();window.Close();System.Windows.Application.Current.Shutdown();};
        timer.Start();System.Windows.Application.Current.Run();
    }
    static IEnumerable<string> Glyphs(Drawing? d){if(d is GlyphRunDrawing g)yield return g.GlyphRun.GlyphTypeface.FontUri.ToString();if(d is DrawingGroup group)foreach(var c in group.Children)foreach(var v in Glyphs(c))yield return v;}
    internal static void Run(string path)
    {
        var w=new MainWindow(new IslandSettings{Animation=false,AlwaysOnTop=false},demo:true);
        w.Show();w.SetDemo(true);w.UpdateLayout();
        var root=(FrameworkElement)w.Content;
        root.Measure(new System.Windows.Size(w.Width,w.Height));root.Arrange(new Rect(0,0,w.Width,w.Height));root.UpdateLayout();
        var results=new Dictionary<string,object>();
        foreach(var name in new[]{"ExpandedView","ExpandedCoverVisual","SongTitle","ArtistName","ExpandedLyric","SecondaryLyric","PlaybackStatus","ElapsedTime","TotalTime","PlaybackProgress","MoveButton","FavoriteButton","PreviousButton","PlayButton","NextButton","Toolbar","CompactView","LyricViewport","CurrentLyric","ActivityOne","ActivityTwo","ActivityThree"})
        {
            var e=(FrameworkElement)w.FindName(name);var p=e.TransformToAncestor(root).Transform(new System.Windows.Point());
            var typeface=e is TextBlock tb?new Typeface(tb.FontFamily,tb.FontStyle,tb.FontWeight,tb.FontStretch):null;GlyphTypeface? glyph=null;if(typeface is not null)typeface.TryGetGlyphTypeface(out glyph);results[name]=new{runs=Glyphs(VisualTreeHelper.GetDrawing(e)).ToArray(),glyph=glyph?.FontUri.ToString(),x=p.X,y=p.Y,width=e.ActualWidth,height=e.ActualHeight,opacity=e.Opacity,enabled=e.IsEnabled,baseline=(e as TextBlock)?.BaselineOffset,fontSize=(e as TextBlock)?.FontSize,font=(e as TextBlock)?.FontFamily.ToString(),text=(e as TextBlock)?.Text,foreground=(e as TextBlock)?.Foreground.ToString()};
        }
        File.WriteAllText(path,JsonSerializer.Serialize(results,new JsonSerializerOptions{WriteIndented=true}));w.Close();
    }
}



