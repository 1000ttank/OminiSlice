using System;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public sealed class OmniSlicePreviewProbe : Form {
  [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] struct Properties { public uint Flags;public Rect Destination,Source;public byte Opacity;[MarshalAs(UnmanagedType.Bool)]public bool Visible;[MarshalAs(UnmanagedType.Bool)]public bool ClientOnly; }
  [StructLayout(LayoutKind.Sequential)] struct Point { public int X,Y; }
  [DllImport("dwmapi.dll")] static extern int DwmRegisterThumbnail(IntPtr destination,IntPtr source,out IntPtr thumbnail);
  [DllImport("dwmapi.dll")] static extern int DwmUpdateThumbnailProperties(IntPtr thumbnail,ref Properties properties);
  [DllImport("dwmapi.dll")] static extern int DwmUnregisterThumbnail(IntPtr thumbnail);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
  [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
  [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd,uint flags);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
  [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int x1,int y1,int x2,int y2);
  [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr hwnd,IntPtr region);
  [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr value);
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  protected override bool ShowWithoutActivation { get { return true; } }
  readonly IntPtr source;
  readonly string directory;
  IntPtr thumbnail;
  int phase;
  Point originalCursor;
  readonly Timer timer=new Timer();
  OmniSlicePreviewProbe(long sourceWindow,string directory) {
    source=new IntPtr(sourceWindow);this.directory=directory;
    Text="OmniSlice DWM Preview Probe";BackColor=Color.FromArgb(23,33,41);ClientSize=new Size(400,280);
    StartPosition=FormStartPosition.Manual;Location=new System.Drawing.Point(800,80);TopMost=true;ShowInTaskbar=false;
    timer.Interval=650;timer.Tick+=Tick;
  }
  protected override void OnShown(EventArgs e) {
    base.OnShown(e);GetCursorPos(out originalCursor);
    SetWindowPos(source,new IntPtr(-1),100,80,0,0,0x11);SetCursorPos(130,110);
    Marshal.ThrowExceptionForHR(DwmRegisterThumbnail(Handle,source,out thumbnail));
    var properties=new Properties { Flags=1|4|8|16,Destination=new Rect{Right=400,Bottom=280},Opacity=255,Visible=true,ClientOnly=true };
    Marshal.ThrowExceptionForHR(DwmUpdateThumbnailProperties(thumbnail,ref properties));timer.Start();
  }
  void SaveCapture(string name) { SetWindowPos(Handle,new IntPtr(-1),0,0,0,0,0x13);var origin=PointToScreen(System.Drawing.Point.Empty);foreach(var point in new[]{new Point{X=origin.X+2,Y=origin.Y+2},new Point{X=origin.X+398,Y=origin.Y+278},new Point{X=origin.X+200,Y=origin.Y+140}})if(GetAncestor(WindowFromPoint(point),2)!=Handle)throw new Exception("Owned preview probe is occluded; capture cancelled");using(var bitmap=new Bitmap(400,280)) { using(var graphics=Graphics.FromImage(bitmap)) { graphics.CopyFromScreen(origin.X,origin.Y,0,0,bitmap.Size); } bitmap.Save(System.IO.Path.Combine(directory,name),System.Drawing.Imaging.ImageFormat.Png); }Rect rect;GetWindowRect(source,out rect);Point cursor;GetCursorPos(out cursor);var region=CreateRectRgn(0,0,0,0);int kind=GetWindowRgn(source,region);DeleteObject(region);System.IO.File.WriteAllText(System.IO.Path.Combine(directory,name+".json"),"{\"region\":"+kind+",\"cursor\":["+cursor.X+","+cursor.Y+"],\"rect\":["+rect.Left+","+rect.Top+","+rect.Right+","+rect.Bottom+"]}"); }
  void Tick(object sender,EventArgs e) {
    phase++;
    if(phase==1)SaveCapture("dwm-before.png");
    if(phase==2)SetCursorPos(10,10);
    if(phase==3)SaveCapture("dwm-hidden.png");
    if(phase==4){timer.Stop();Close();}
  }
  protected override void OnFormClosed(FormClosedEventArgs e) { if(thumbnail!=IntPtr.Zero)DwmUnregisterThumbnail(thumbnail);SetCursorPos(originalCursor.X,originalCursor.Y);timer.Dispose();base.OnFormClosed(e); }
  public static void Run(long sourceWindow,string directory) { if(sourceWindow<=0)throw new ArgumentOutOfRangeException("sourceWindow");SetThreadDpiAwarenessContext(new IntPtr(-4));Application.EnableVisualStyles();using(var probe=new OmniSlicePreviewProbe(sourceWindow,directory))Application.Run(probe); }
}
