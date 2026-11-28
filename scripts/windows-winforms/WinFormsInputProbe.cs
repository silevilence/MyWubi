using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows.Forms;

public static class WinFormsInputProbe
{
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern IntPtr GetFocus();
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags, int x, int y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string windowClass, string title);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out int pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern IntPtr GetModuleHandle(string name);

    // The first five ITfThreadMgr methods in msctf.h, through GetFocus.
    [ComImport, Guid("AA80E801-2021-11D2-93E0-0060B067B86E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface ITfThreadMgr {
        void Activate(out uint client);
        void Deactivate();
        void CreateDocumentMgr(out IntPtr document);
        void EnumDocumentMgrs(out IntPtr documents);
        [PreserveSig] int GetFocus(out IntPtr document);
    }

    [DllImport("msctf.dll")] static extern int TF_GetThreadMgr(out ITfThreadMgr manager);

    static bool HasDocumentFocus()
    {
        ITfThreadMgr manager;
        Marshal.ThrowExceptionForHR(TF_GetThreadMgr(out manager));
        if (manager == null) return false;
        IntPtr document;
        Marshal.ThrowExceptionForHR(manager.GetFocus(out document));
        if (document == IntPtr.Zero) return false;
        Marshal.Release(document);
        return true;
    }

    // Calling Focus() on an already-focused EDIT does not deliver WM_SETFOCUS.
    // Give this form's editor a real focus transition after TSF activation.
    public static async Task FocusEditor(TextBox editor, Button focusButton)
    {
        if (!focusButton.Focus()) throw new InvalidOperationException("Could not focus the test form's button");
        await Task.Yield();
        if (!editor.Focus()) throw new InvalidOperationException("Could not return focus to the test editor");
        await WaitFor(() => Focused(editor) && HasDocumentFocus(), "Expected TSF document focus on the editor");
    }

    static bool Focused(TextBox editor)
    {
        return GetForegroundWindow() == editor.FindForm().Handle && GetFocus() == editor.Handle;
    }

    static void Press(TextBox editor, byte key, bool windowsKey)
    {
        if (!Focused(editor)) throw new InvalidOperationException("Test editor lost keyboard focus");
        if (windowsKey) keybd_event(0x5B, 0, 1, UIntPtr.Zero);
        keybd_event(key, 0, 0, UIntPtr.Zero);
        keybd_event(key, 0, 2, UIntPtr.Zero);
        if (windowsKey) keybd_event(0x5B, 0, 3, UIntPtr.Zero);
    }

    static async Task WaitFor(Func<bool> predicate, string expectation)
    {
        var elapsed = Stopwatch.StartNew();
        while (!predicate()) {
            if (elapsed.ElapsedMilliseconds >= 5000) throw new TimeoutException(expectation);
            await Task.Delay(40);
        }
    }

    static int Language() { return InputLanguage.CurrentInputLanguage.Culture.LCID; }

    static async Task SelectLanguage(TextBox editor, int language)
    {
        if (Language() != language) {
            Press(editor, 0x20, true); // Win+Space with the test form in the foreground.
            await WaitFor(() => Language() == language && Focused(editor),
                          "Expected input language " + language + " on the editor thread");
        }
    }

    static bool CandidateVisible(int hostPid)
    {
        var window = IntPtr.Zero;
        while ((window = FindWindowEx(IntPtr.Zero, window, "MyWubiCandidateWindow", null)) != IntPtr.Zero) {
            int owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == hostPid && IsWindowVisible(window)) return true;
        }
        return false;
    }

    public static async Task<string> Run(TextBox editor, Button focusButton)
    {
        var form = editor.FindForm();
        try {
            await Task.Yield(); // Let ShowDialog finish its initial focus processing.
            if (!Focused(editor)) {
                var point = editor.PointToScreen(new Point(editor.ClientSize.Width / 2, editor.ClientSize.Height / 2));
                Cursor.Position = point;
                if (WindowFromPoint(point) != editor.Handle) throw new InvalidOperationException("Test editor is covered");
                mouse_event(2, 0, 0, 0, UIntPtr.Zero);
                mouse_event(4, 0, 0, 0, UIntPtr.Zero);
            }
            await WaitFor(() => Focused(editor), "Expected focused WinForms TextBox");
            await SelectLanguage(editor, 0x0409);
            int pid = Process.GetCurrentProcess().Id;
            Press(editor, 0x41, false);
            await WaitFor(() => editor.Text == "a" && !CandidateVisible(pid),
                          "Expected literal a without a MyWubi candidate");
            Press(editor, 0x20, false);
            await WaitFor(() => editor.Text == "a ", "US keyboard should produce a literal letter and space");
            var results = new List<string> { "US [" + editor.Text + "]" };
            editor.Clear();

            await SelectLanguage(editor, 0x0804);
            await FocusEditor(editor, focusButton);
            await WaitFor(() => GetModuleHandle("im_engine_x86.dll") != IntPtr.Zero,
                          "MyWubi x86 was not loaded in the WinForms host");
            foreach (string expected in new[] { "\u5de5", "\u5de5\u5de5" }) {
                Press(editor, 0x41, false);
                await WaitFor(() => CandidateVisible(pid), "Expected MyWubi candidate in the host process");
                Press(editor, 0x20, false);
                await WaitFor(() => !CandidateVisible(pid) && editor.Text == expected &&
                              editor.SelectionStart == expected.Length && editor.SelectionLength == 0 &&
                              Focused(editor), "Expected committed text and collapsed caret: " + expected);
                results.Add(String.Format("MyWubi x86 [{0}] caret={1} selected={2}",
                                         editor.Text, editor.SelectionStart, editor.SelectionLength));
            }
            return "PASS: " + String.Join(", ", results);
        } finally {
            form.Close();
        }
    }
}
