using System;
using System.Runtime.InteropServices;
public static class OmniSliceProbe {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string className, string title);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
  [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
  [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
  [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
  [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd,uint flags);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd,System.Text.StringBuilder name,int count);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint process);
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  static OmniSliceProbe() { SetThreadDpiAwarenessContext(new IntPtr(-4)); }
  [DllImport("user32.dll")] static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);
  [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr hwnd, IntPtr region);
  [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int x1,int y1,int x2,int y2);
  [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr value);
  [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr hwnd,int attribute,out int value,int size);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint dx,uint dy,uint data,UIntPtr extra);
  [StructLayout(LayoutKind.Sequential)] struct MouseInput { public int X,Y;public uint Data,Flags,Time;public UIntPtr Extra; }
  [StructLayout(LayoutKind.Sequential)] struct Input { public uint Type;public MouseInput Mouse; }
  [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,Input[] inputs,int size);
  static void Button(uint flags) { var input=new Input { Type=0,Mouse=new MouseInput { Flags=flags } };if(SendInput(1,new[]{input},Marshal.SizeOf(typeof(Input)))!=1)throw new Exception("Native SendInput failed: "+Marshal.GetLastWin32Error()); }
  [DllImport("user32.dll")] static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
  public static string State(string id) {
    var hwnd=FindWindow(null,"OmniSlice — "+id);
    if(hwnd==IntPtr.Zero) throw new Exception("Owned OmniSlice window not found: "+id);
    Rect rect;GetWindowRect(hwnd,out rect);
    Rect client;GetClientRect(hwnd,out client);Point origin=new Point();ClientToScreen(hwnd,ref origin);
    var region=CreateRectRgn(0,0,0,0);var kind=GetWindowRgn(hwnd,region);DeleteObject(region);
    int force,bitmap;int forceResult=DwmGetWindowAttribute(hwnd,7,out force,4);int bitmapResult=DwmGetWindowAttribute(hwnd,10,out bitmap,4);
    uint process;GetWindowThreadProcessId(hwnd,out process);
    Point cursor;GetCursorPos(out cursor);
    return "{\"pid\":"+process+",\"hwnd\":"+hwnd.ToInt64()+",\"foreground\":"+GetForegroundWindow().ToInt64()+",\"region\":"+kind+",\"styles\":"+GetWindowLongPtr(hwnd,-20).ToInt64()+",\"forceIconic\":"+force+",\"forceResult\":"+forceResult+",\"bitmapResult\":"+bitmapResult+",\"hasBitmap\":"+bitmap+",\"screen\":["+GetSystemMetrics(0)+","+GetSystemMetrics(1)+"],\"cursor\":["+cursor.X+","+cursor.Y+"],\"client\":["+origin.X+","+origin.Y+","+client.Right+","+client.Bottom+"],\"rect\":["+rect.Left+","+rect.Top+","+rect.Right+","+rect.Bottom+"]}";
  }
  public static void Click(int x,int y) { SetCursorPos(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);mouse_event(4,0,0,0,UIntPtr.Zero); }
  public static string MoveAndState(string id,int x,int y) { bool moved=SetCursorPos(x,y);System.Threading.Thread.Sleep(500);return "{\"moved\":"+(moved?"true":"false")+",\"state\":"+State(id)+"}"; }
  public static string Hit(int x,int y) { var hwnd=WindowFromPoint(new Point { X=x,Y=y });uint process;GetWindowThreadProcessId(hwnd,out process);var name=new System.Text.StringBuilder(256);GetClassName(hwnd,name,256);return "{\"hwnd\":"+hwnd.ToInt64()+",\"root\":"+GetAncestor(hwnd,2).ToInt64()+",\"pid\":"+process+",\"class\":\""+name+"\"}"; }
  public static void Place(string id) { var hwnd=FindWindow(null,"OmniSlice — "+id);if(hwnd==IntPtr.Zero)throw new Exception("Owned window missing");if(!SetWindowPos(hwnd,IntPtr.Zero,100,80,0,0,0x15))throw new Exception("Unable to position owned window"); }
  public static string HoverRoundTrip(string id) {
    var hwnd=FindWindow(null,"OmniSlice — "+id);Rect rect;GetWindowRect(hwnd,out rect);
    bool outMoved=SetCursorPos(10,10);System.Threading.Thread.Sleep(350);string hidden=State(id);
    bool inMoved=SetCursorPos(rect.Left+30,rect.Top+30);System.Threading.Thread.Sleep(350);string visible=State(id);
    return "{\"outMoved\":"+(outMoved?"true":"false")+",\"inMoved\":"+(inMoved?"true":"false")+",\"hidden\":"+hidden+",\"visible\":"+visible+"}";
  }
  public static string Drag(string id,int x1,int y1,int x2,int y2) { var target=FindWindow(null,"OmniSlice — "+id);if(target==IntPtr.Zero)throw new Exception("Owned target missing");if(!SetWindowPos(target,new IntPtr(-1),0,0,0,0,0x13))throw new Exception("Unable to raise owned target");if(!SetCursorPos(x1,y1))throw new Exception("Unable to move native pointer");string start=Hit(x1,y1);if(GetAncestor(WindowFromPoint(new Point {X=x1,Y=y1}),2)!=target)throw new Exception("Owned target is occluded; native input cannot be tested");Button(2);System.Threading.Thread.Sleep(100);if(!SetCursorPos(x2,y2))throw new Exception("Unable to move native pointer");System.Threading.Thread.Sleep(100);Button(4);Point cursor;GetCursorPos(out cursor);return "{\"startHit\":"+start+",\"end\":["+cursor.X+","+cursor.Y+"]}"; }
  public static void Recover() { byte[] keys={0x11,0x12,0x10,0x4f};foreach(var key in keys)keybd_event(key,0,0,UIntPtr.Zero);Array.Reverse(keys);foreach(var key in keys)keybd_event(key,0,2,UIntPtr.Zero); }
}
