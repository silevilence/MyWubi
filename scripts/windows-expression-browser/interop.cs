using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using Accessibility;

public sealed class AccessibleSnapshot { internal IAccessible Value; public AccessibleSnapshot(IAccessible value) { Value=value; } }
public static class BrowserAutomation {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
    [DllImport("user32.dll")] static extern void keybd_event(byte vk,byte scan,uint flags,UIntPtr extra);
    [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd,uint id,ref Guid iid,[MarshalAs(UnmanagedType.Interface)] out IAccessible accessible);
    [StructLayout(LayoutKind.Sequential)] struct Rect {public int L,T,R,B;}
    public static IntPtr Palette() {var w=FindWindow("MyWubiCandidateWindow",null); return IsWindowVisible(w)?w:IntPtr.Zero;}
    public static AccessibleSnapshot Snapshot() {
        var hwnd=Palette(); if(hwnd==IntPtr.Zero) throw new Exception("Palette not visible");
        var iid=typeof(IAccessible).GUID; IAccessible result;
        Marshal.ThrowExceptionForHR(AccessibleObjectFromWindow(hwnd,unchecked((uint)-4),ref iid,out result));
        return new AccessibleSnapshot(result);
    }
    public static string[] Names(AccessibleSnapshot snapshot) { var a=snapshot.Value;
        var names=new List<string>();for(int i=1;i<=a.accChildCount;i++)names.Add(a.get_accName(i));return names.ToArray();
    }
    public static int Find(AccessibleSnapshot snapshot,string prefix) { var a=snapshot.Value;
        for(int i=1;i<=a.accChildCount;i++)if(a.get_accName(i).StartsWith(prefix,StringComparison.Ordinal))return i;
        throw new Exception("Missing control: "+prefix+"; visible="+String.Join(" | ",Names(snapshot)));
    }
    public static void Invoke(AccessibleSnapshot snapshot,int id) {snapshot.Value.accDoDefaultAction(id);}
    public static void Click(AccessibleSnapshot snapshot,int id) { var a=snapshot.Value;
        int x,y,w,h;a.accLocation(out x,out y,out w,out h,id);SetCursorPos(x+w/2,y+h/2);
        mouse_event(2,0,0,0,UIntPtr.Zero);mouse_event(4,0,0,0,UIntPtr.Zero);
    }
    public static void Key(byte vk) {keybd_event(vk,0,0,UIntPtr.Zero);keybd_event(vk,0,2,UIntPtr.Zero);}
    public static void CtrlA() {keybd_event(0x11,0,0,UIntPtr.Zero);Key(0x41);keybd_event(0x11,0,2,UIntPtr.Zero);}
    public static void Wheel(int delta) {var a=Snapshot().Value;int x,y,w,h;a.accLocation(out x,out y,out w,out h,0);SetCursorPos(x+w/2,y+h/2);mouse_event(0x800,0,0,unchecked((uint)delta),UIntPtr.Zero);}
    public static void Capture(string path) {
        Rect r;GetWindowRect(Palette(),out r);
        using(var b=new Bitmap(r.R-r.L,r.B-r.T)) {using(var g=Graphics.FromImage(b))g.CopyFromScreen(r.L,r.T,0,0,b.Size);b.Save(path,ImageFormat.Png);}
    }
}
